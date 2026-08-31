//! Private lopdf integration and PDF page-tree normalization.

use std::collections::HashSet;

use lopdf::{Dictionary, Document, LoadOptions, Object, ObjectId};
use orbitrelay_asset::AssetId;
use orbitrelay_document::{PageDisplayGeometry, PageRotation};

use crate::{PdfDocumentMetadata, PdfError, PdfInspectionLimits, PdfPageMetadata};

const PAGE_TREE_DEPTH_LIMIT: usize = 256;

/// Parses bytes into library-neutral PDF metadata.
pub(crate) fn inspect_bytes(
    asset_id: AssetId,
    bytes: &[u8],
    limits: PdfInspectionLimits,
) -> Result<PdfDocumentMetadata, PdfError> {
    if contains_encrypt_marker(bytes) {
        return Err(PdfError::EncryptedUnsupported);
    }

    let decompressed_limit =
        usize::try_from(limits.max_decompressed_stream_bytes()).unwrap_or(usize::MAX);
    let document = Document::load_mem_with_options(
        bytes,
        LoadOptions {
            strict: false,
            max_decompressed_size: Some(decompressed_limit),
            ..Default::default()
        },
    )
    .map_err(|error| map_parser_error(error, limits))?;

    // This also catches encrypted files whose marker was hidden in an unusual
    // representation. It is intentionally checked after parsing as a second
    // guard, although the preflight above prevents lopdf password attempts for
    // ordinary encrypted PDFs.
    if document.is_encrypted() || document.was_encrypted() {
        return Err(PdfError::EncryptedUnsupported);
    }

    let title = extract_title(&document);
    let pages = collect_pages(&document, limits)?;
    if pages.is_empty() {
        return Err(PdfError::InvalidPageTree);
    }

    let mut descriptors = Vec::with_capacity(pages.len());
    for (index, page) in pages.into_iter().enumerate() {
        let page_index = u32::try_from(index).map_err(|_| PdfError::PageIndexOverflow)?;
        let geometry = page_geometry(&document, page, page_index)?;
        descriptors.push(PdfPageMetadata::new(page_index, geometry));
    }

    Ok(PdfDocumentMetadata::new(asset_id, title, descriptors))
}

#[derive(Clone, Default)]
struct InheritedAttributes {
    media_box: Option<Object>,
    crop_box: Option<Object>,
    rotate: Option<Object>,
}

struct RawPage {
    attributes: InheritedAttributes,
}

fn collect_pages(
    document: &Document,
    limits: PdfInspectionLimits,
) -> Result<Vec<RawPage>, PdfError> {
    let catalog = document.catalog().map_err(|_| PdfError::InvalidPageTree)?;
    let pages_id = catalog
        .get(b"Pages")
        .and_then(Object::as_reference)
        .map_err(|_| PdfError::InvalidPageTree)?;

    let mut walker = PageTreeWalker {
        document,
        limits,
        pages: Vec::new(),
        active: HashSet::new(),
        seen: HashSet::new(),
    };
    walker.visit(pages_id, InheritedAttributes::default(), 0)?;
    Ok(walker.pages)
}

struct PageTreeWalker<'a> {
    document: &'a Document,
    limits: PdfInspectionLimits,
    pages: Vec<RawPage>,
    active: HashSet<ObjectId>,
    seen: HashSet<ObjectId>,
}

impl PageTreeWalker<'_> {
    fn visit(
        &mut self,
        node_id: ObjectId,
        inherited: InheritedAttributes,
        depth: usize,
    ) -> Result<(), PdfError> {
        if depth > PAGE_TREE_DEPTH_LIMIT
            || !self.active.insert(node_id)
            || !self.seen.insert(node_id)
        {
            return Err(PdfError::InvalidPageTree);
        }

        let result = (|| {
            let dictionary = self
                .document
                .get_dictionary(node_id)
                .map_err(|_| PdfError::InvalidPageTree)?;
            let type_name = dictionary
                .get_deref(b"Type", self.document)
                .and_then(Object::as_name)
                .map_err(|_| PdfError::InvalidPageTree)?;

            let mut effective = inherited;
            inherit(&mut effective.media_box, dictionary, b"MediaBox");
            inherit(&mut effective.crop_box, dictionary, b"CropBox");
            inherit(&mut effective.rotate, dictionary, b"Rotate");

            match type_name {
                b"Page" => {
                    if self.pages.len()
                        >= usize::try_from(self.limits.max_pages()).unwrap_or(usize::MAX)
                    {
                        return Err(PdfError::PageLimitExceeded {
                            max_pages: self.limits.max_pages(),
                        });
                    }
                    self.pages.push(RawPage {
                        attributes: resolve_attributes(self.document, effective),
                    });
                    Ok(())
                }
                b"Pages" => {
                    let kids = dictionary
                        .get_deref(b"Kids", self.document)
                        .and_then(Object::as_array)
                        .map_err(|_| PdfError::InvalidPageTree)?;
                    if kids.is_empty() {
                        return Err(PdfError::InvalidPageTree);
                    }
                    let kid_ids = kids
                        .iter()
                        .map(|kid| kid.as_reference().map_err(|_| PdfError::InvalidPageTree))
                        .collect::<Result<Vec<_>, _>>()?;
                    for kid_id in kid_ids {
                        self.visit(kid_id, effective.clone(), depth + 1)?;
                    }
                    Ok(())
                }
                _ => Err(PdfError::InvalidPageTree),
            }
        })();

        self.active.remove(&node_id);
        result
    }
}

fn resolve_attributes(document: &Document, attributes: InheritedAttributes) -> InheritedAttributes {
    InheritedAttributes {
        media_box: resolve_object(document, attributes.media_box),
        crop_box: resolve_object(document, attributes.crop_box),
        rotate: resolve_object(document, attributes.rotate),
    }
}

fn resolve_object(document: &Document, object: Option<Object>) -> Option<Object> {
    object.and_then(|value| {
        document
            .dereference(&value)
            .ok()
            .map(|(_, resolved)| resolved.clone())
    })
}

fn inherit(slot: &mut Option<Object>, dictionary: &Dictionary, key: &[u8]) {
    if let Ok(value) = dictionary.get(key) {
        *slot = Some(value.clone());
    }
}

fn page_geometry(
    document: &Document,
    page: RawPage,
    page_index: u32,
) -> Result<PageDisplayGeometry, PdfError> {
    let media_box = page
        .attributes
        .media_box
        .as_ref()
        .ok_or(PdfError::MissingMediaBox { page_index })?;
    let media =
        parse_box(document, media_box).ok_or(PdfError::InvalidPageGeometry { page_index })?;
    let visible = page
        .attributes
        .crop_box
        .as_ref()
        .and_then(|object| parse_box(document, object))
        .unwrap_or(media);
    let rotation = parse_rotation(document, page.attributes.rotate.as_ref(), page_index)?;
    let (width, height) = match rotation {
        PageRotation::Deg0 | PageRotation::Deg180 => (visible.width, visible.height),
        PageRotation::Deg90 | PageRotation::Deg270 => (visible.height, visible.width),
    };
    PageDisplayGeometry::new(width, height, rotation)
        .map_err(|_| PdfError::InvalidPageGeometry { page_index })
}

struct BoxDimensions {
    width: f64,
    height: f64,
}

fn parse_box(document: &Document, object: &Object) -> Option<BoxDimensions> {
    let array = object.as_array().ok()?;
    if array.len() != 4 {
        return None;
    }
    let x0 = number(document, &array[0])?;
    let y0 = number(document, &array[1])?;
    let x1 = number(document, &array[2])?;
    let y1 = number(document, &array[3])?;
    let width = (x1 - x0).abs();
    let height = (y1 - y0).abs();
    if width.is_finite() && width > 0.0 && height.is_finite() && height > 0.0 {
        Some(BoxDimensions { width, height })
    } else {
        None
    }
}

fn number(document: &Document, object: &Object) -> Option<f64> {
    let object = document.dereference(object).ok().map(|(_, value)| value)?;
    object
        .as_float()
        .ok()
        .map(f64::from)
        .filter(|value| value.is_finite())
}

fn parse_rotation(
    document: &Document,
    object: Option<&Object>,
    page_index: u32,
) -> Result<PageRotation, PdfError> {
    let raw = object
        .and_then(|value| {
            document
                .dereference(value)
                .ok()
                .map(|(_, resolved)| resolved)
        })
        .map_or(Ok(0_i64), Object::as_i64)
        .map_err(|_| PdfError::InvalidRotation {
            page_index,
            degrees: 0,
        })?;
    let normalized = raw.rem_euclid(360);
    if normalized % 90 != 0 {
        return Err(PdfError::InvalidRotation {
            page_index,
            degrees: raw,
        });
    }
    PageRotation::from_degrees(normalized as u16).map_err(|_| PdfError::InvalidRotation {
        page_index,
        degrees: raw,
    })
}

fn extract_title(document: &Document) -> Option<String> {
    let info_object = document
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|object| document.dereference(object).ok().map(|(_, value)| value));
    let dictionary = info_object?.as_dict().ok()?;
    let title = dictionary.get_deref(b"Title", document).ok()?;
    let decoded = lopdf::decode_text_string(title).ok()?;
    let trimmed = decoded.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

fn contains_encrypt_marker(bytes: &[u8]) -> bool {
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => skip_comment(bytes, &mut index),
            b'(' => skip_literal_string(bytes, &mut index),
            b'<' if bytes.get(index + 1) != Some(&b'<') => skip_hex_string(bytes, &mut index),
            b'/' if bytes[index..].starts_with(b"/Encrypt") => {
                let end = index + b"/Encrypt".len();
                if bytes
                    .get(end)
                    .is_none_or(|next| next.is_ascii_whitespace() || b"()<>[]{}/%".contains(next))
                {
                    return true;
                }
                index += 1;
            }
            _ => index += 1,
        }
    }
    false
}

fn skip_comment(bytes: &[u8], index: &mut usize) {
    while *index < bytes.len() && !matches!(bytes[*index], b'\r' | b'\n') {
        *index += 1;
    }
}

fn skip_literal_string(bytes: &[u8], index: &mut usize) {
    let mut depth = 0_u32;
    while *index < bytes.len() {
        match bytes[*index] {
            b'\\' => *index = (*index + 2).min(bytes.len()),
            b'(' => {
                depth += 1;
                *index += 1;
            }
            b')' => {
                depth = depth.saturating_sub(1);
                *index += 1;
                if depth == 0 {
                    break;
                }
            }
            _ => *index += 1,
        }
    }
}

fn skip_hex_string(bytes: &[u8], index: &mut usize) {
    *index += 1;
    while *index < bytes.len() {
        let byte = bytes[*index];
        *index += 1;
        if byte == b'>' {
            break;
        }
    }
}

fn map_parser_error(error: lopdf::Error, limits: PdfInspectionLimits) -> PdfError {
    match error {
        lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded { .. }) => {
            PdfError::ParserResourceLimitExceeded {
                max_bytes: limits.max_decompressed_stream_bytes(),
            }
        }
        lopdf::Error::InvalidPassword
        | lopdf::Error::UnsupportedSecurityHandler(_)
        | lopdf::Error::Decryption(_) => PdfError::EncryptedUnsupported,
        _ => PdfError::InvalidPdf,
    }
}
