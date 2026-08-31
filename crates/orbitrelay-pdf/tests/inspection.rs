use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use lopdf::{
    dictionary, Dictionary, Document, EncryptionState, EncryptionVersion, Object, StringFormat,
};
use orbitrelay_asset::{AssetId, ContentHash, SourceAssetDescriptor};
use orbitrelay_asset_runtime::{AssetByteChunk, AssetByteRange, AssetReader, MemoryAssetStore};
use orbitrelay_document::PageRotation;
use orbitrelay_pdf::{PdfError, PdfInspectionLimits, PdfInspector};
use sha2::{Digest, Sha256};

fn pdf_bytes(
    root_attributes: Dictionary,
    pages: Vec<(u32, Dictionary)>,
    order: &[u32],
    title: Option<Object>,
) -> Vec<u8> {
    let pages_id = (2, 0);
    let mut document = Document::with_version("1.7");
    document.max_id = 20;
    let mut root = root_attributes;
    root.set("Type", "Pages");
    root.set(
        "Kids",
        Object::Array(order.iter().map(|id| Object::Reference((*id, 0))).collect()),
    );
    root.set("Count", order.len() as i64);
    document.objects.insert(pages_id, Object::Dictionary(root));

    let catalog_id = (1, 0);
    document.objects.insert(
        catalog_id,
        dictionary!("Type" => "Catalog", "Pages" => Object::Reference(pages_id)).into(),
    );
    document.trailer.set("Root", Object::Reference(catalog_id));
    for (id, page) in pages {
        document.objects.insert((id, 0), Object::Dictionary(page));
    }
    if let Some(title) = title {
        let info_id = (3, 0);
        document
            .objects
            .insert(info_id, dictionary!("Title" => title).into());
        document.trailer.set("Info", Object::Reference(info_id));
    }

    let mut bytes = Vec::new();
    document.save_to(&mut bytes).expect("fixture should save");
    bytes
}

fn page(
    parent: (u32, u16),
    media: Option<Object>,
    crop: Option<Object>,
    rotate: Option<Object>,
) -> Dictionary {
    let mut page = dictionary!("Type" => "Page", "Parent" => Object::Reference(parent));
    if let Some(media) = media {
        page.set("MediaBox", media);
    }
    if let Some(crop) = crop {
        page.set("CropBox", crop);
    }
    if let Some(rotate) = rotate {
        page.set("Rotate", rotate);
    }
    page
}

fn box_object(x0: i64, y0: i64, x1: i64, y1: i64) -> Object {
    Object::Array(vec![x0.into(), y0.into(), x1.into(), y1.into()])
}

fn descriptor(asset_id: AssetId, bytes: &[u8], media_type: &str) -> SourceAssetDescriptor {
    let digest = Sha256::digest(bytes);
    let mut hash = [0_u8; 32];
    hash.copy_from_slice(&digest);
    SourceAssetDescriptor::new(
        asset_id,
        media_type,
        bytes.len() as u64,
        ContentHash::from_bytes(hash),
        Some("fixture.pdf".to_owned()),
    )
    .expect("fixture metadata should be valid")
}

fn inspector_for(
    bytes: &[u8],
    media_type: &str,
    limits: PdfInspectionLimits,
) -> (PdfInspector, AssetId) {
    let store = MemoryAssetStore::new();
    let asset_id = AssetId::new();
    store
        .insert_verified(
            descriptor(asset_id.clone(), bytes, media_type),
            Bytes::copy_from_slice(bytes),
        )
        .expect("fixture should insert");
    let shared = Arc::new(store);
    (PdfInspector::new(shared.clone(), shared, limits), asset_id)
}

#[tokio::test]
async fn parses_single_page_geometry_and_asset_identity() {
    let bytes = pdf_bytes(
        Dictionary::new(),
        vec![(
            10,
            page((2, 0), Some(box_object(0, 0, 612, 792)), None, None),
        )],
        &[10],
        None,
    );
    let (inspector, asset_id) =
        inspector_for(&bytes, "application/pdf", PdfInspectionLimits::default());
    let metadata = inspector
        .inspect(&asset_id)
        .await
        .expect("PDF should parse");
    assert_eq!(metadata.asset_id(), &asset_id);
    assert_eq!(metadata.page_count(), 1);
    assert_eq!(metadata.pages()[0].page_index(), 0);
    assert_eq!(metadata.pages()[0].display_geometry().width(), 612.0);
    assert_eq!(metadata.pages()[0].display_geometry().height(), 792.0);
    assert_eq!(
        metadata.pages()[0].display_geometry().rotation(),
        PageRotation::Deg0
    );
}

#[tokio::test]
async fn uses_crop_box_nonzero_origin_and_rotation_dimensions() {
    let bytes = pdf_bytes(
        Dictionary::new(),
        vec![(
            10,
            page(
                (2, 0),
                Some(box_object(0, 0, 612, 792)),
                Some(box_object(50, 100, 550, 700)),
                Some(90.into()),
            ),
        )],
        &[10],
        None,
    );
    let (inspector, asset_id) = inspector_for(
        &bytes,
        "application/octet-stream",
        PdfInspectionLimits::default(),
    );
    let geometry = inspector.inspect(&asset_id).await.unwrap().pages()[0].display_geometry();
    assert_eq!(geometry.width(), 600.0);
    assert_eq!(geometry.height(), 500.0);
    assert_eq!(geometry.rotation(), PageRotation::Deg90);
}

#[tokio::test]
async fn inherits_media_crop_and_rotation_from_pages_node() {
    let root = dictionary!(
        "MediaBox" => box_object(10, 20, 610, 820),
        "CropBox" => box_object(10, 20, 510, 620),
        "Rotate" => 270
    );
    let bytes = pdf_bytes(
        root,
        vec![(10, page((2, 0), None, None, None))],
        &[10],
        None,
    );
    let (inspector, asset_id) =
        inspector_for(&bytes, "application/pdf", PdfInspectionLimits::default());
    let geometry = inspector.inspect(&asset_id).await.unwrap().pages()[0].display_geometry();
    assert_eq!(geometry.width(), 600.0);
    assert_eq!(geometry.height(), 500.0);
    assert_eq!(geometry.rotation(), PageRotation::Deg270);
}

#[tokio::test]
async fn follows_page_tree_order_instead_of_object_numbers() {
    let bytes = pdf_bytes(
        Dictionary::new(),
        vec![
            (
                20,
                page((2, 0), Some(box_object(0, 0, 100, 200)), None, None),
            ),
            (
                5,
                page((2, 0), Some(box_object(0, 0, 300, 400)), None, None),
            ),
        ],
        &[20, 5],
        None,
    );
    let (inspector, asset_id) =
        inspector_for(&bytes, "application/pdf", PdfInspectionLimits::default());
    let metadata = inspector.inspect(&asset_id).await.unwrap();
    let pages = metadata.pages();
    assert_eq!(pages[0].display_geometry().width(), 100.0);
    assert_eq!(pages[1].display_geometry().width(), 300.0);
    assert_eq!(pages[0].page_index(), 0);
    assert_eq!(pages[1].page_index(), 1);
}

#[tokio::test]
async fn normalizes_equivalent_rotations_without_domain_dimension_swap() {
    for (raw, expected) in [
        (0_i64, PageRotation::Deg0),
        (360, PageRotation::Deg0),
        (450, PageRotation::Deg90),
        (-90, PageRotation::Deg270),
        (180, PageRotation::Deg180),
    ] {
        let bytes = pdf_bytes(
            Dictionary::new(),
            vec![(
                10,
                page(
                    (2, 0),
                    Some(box_object(0, 0, 12, 34)),
                    None,
                    Some(raw.into()),
                ),
            )],
            &[10],
            None,
        );
        let (inspector, asset_id) =
            inspector_for(&bytes, "application/pdf", PdfInspectionLimits::default());
        let geometry = inspector.inspect(&asset_id).await.unwrap().pages()[0].display_geometry();
        assert_eq!(geometry.rotation(), expected);
        assert_eq!(
            (geometry.width(), geometry.height()),
            if matches!(expected, PageRotation::Deg90 | PageRotation::Deg270) {
                (34.0, 12.0)
            } else {
                (12.0, 34.0)
            }
        );
    }
}

#[tokio::test]
async fn rejects_invalid_rotation_and_geometry() {
    let invalid_rotation = pdf_bytes(
        Dictionary::new(),
        vec![(
            10,
            page(
                (2, 0),
                Some(box_object(0, 0, 10, 10)),
                None,
                Some(45.into()),
            ),
        )],
        &[10],
        None,
    );
    let (inspector, asset_id) = inspector_for(
        &invalid_rotation,
        "application/pdf",
        PdfInspectionLimits::default(),
    );
    assert!(matches!(
        inspector.inspect(&asset_id).await,
        Err(PdfError::InvalidRotation { .. })
    ));

    let zero_area = pdf_bytes(
        Dictionary::new(),
        vec![(10, page((2, 0), Some(box_object(0, 0, 0, 10)), None, None))],
        &[10],
        None,
    );
    let (inspector, asset_id) = inspector_for(
        &zero_area,
        "application/pdf",
        PdfInspectionLimits::default(),
    );
    assert!(matches!(
        inspector.inspect(&asset_id).await,
        Err(PdfError::InvalidPageGeometry { .. })
    ));

    let missing_media = pdf_bytes(
        Dictionary::new(),
        vec![(10, page((2, 0), None, None, None))],
        &[10],
        None,
    );
    let (inspector, asset_id) = inspector_for(
        &missing_media,
        "application/pdf",
        PdfInspectionLimits::default(),
    );
    assert!(matches!(
        inspector.inspect(&asset_id).await,
        Err(PdfError::MissingMediaBox { page_index: 0 })
    ));

    let non_numeric_media = pdf_bytes(
        Dictionary::new(),
        vec![(
            10,
            page(
                (2, 0),
                Some(Object::Array(vec![
                    0.into(),
                    0.into(),
                    Object::Name(b"wide".to_vec()),
                    10.into(),
                ])),
                None,
                None,
            ),
        )],
        &[10],
        None,
    );
    let (inspector, asset_id) = inspector_for(
        &non_numeric_media,
        "application/pdf",
        PdfInspectionLimits::default(),
    );
    assert!(matches!(
        inspector.inspect(&asset_id).await,
        Err(PdfError::InvalidPageGeometry { page_index: 0 })
    ));
}

#[tokio::test]
async fn enforces_page_limit_before_emitting_metadata() {
    let bytes = pdf_bytes(
        Dictionary::new(),
        vec![
            (10, page((2, 0), Some(box_object(0, 0, 10, 10)), None, None)),
            (11, page((2, 0), Some(box_object(0, 0, 20, 20)), None, None)),
        ],
        &[10, 11],
        None,
    );
    let limits = PdfInspectionLimits::new(64 * 1024, 1, 1024 * 1024);
    let (inspector, asset_id) = inspector_for(&bytes, "application/pdf", limits);
    assert!(matches!(
        inspector.inspect(&asset_id).await,
        Err(PdfError::PageLimitExceeded { max_pages: 1 })
    ));
}

#[tokio::test]
async fn title_is_optional_and_decode_failures_are_non_fatal() {
    let valid = pdf_bytes(
        Dictionary::new(),
        vec![(10, page((2, 0), Some(box_object(0, 0, 10, 10)), None, None))],
        &[10],
        Some(Object::String(
            b"  Lesson  ".to_vec(),
            StringFormat::Literal,
        )),
    );
    let (inspector, asset_id) =
        inspector_for(&valid, "application/pdf", PdfInspectionLimits::default());
    assert_eq!(
        inspector.inspect(&asset_id).await.unwrap().title(),
        Some("Lesson")
    );

    let malformed = pdf_bytes(
        Dictionary::new(),
        vec![(10, page((2, 0), Some(box_object(0, 0, 10, 10)), None, None))],
        &[10],
        Some(Object::String(
            vec![0xfe, 0xff, 0xd8, 0x00],
            StringFormat::Literal,
        )),
    );
    let (inspector, asset_id) = inspector_for(
        &malformed,
        "application/pdf",
        PdfInspectionLimits::default(),
    );
    let metadata = inspector.inspect(&asset_id).await.unwrap();
    assert_eq!(metadata.title(), None);
}

#[tokio::test]
async fn rejects_invalid_pdf_missing_asset_and_size_limit() {
    let store = MemoryAssetStore::new();
    let unknown = AssetId::new();
    let shared = Arc::new(store.clone());
    let inspector = PdfInspector::new(shared.clone(), shared, PdfInspectionLimits::default());
    assert!(matches!(
        inspector.inspect(&unknown).await,
        Err(PdfError::AssetNotFound { .. })
    ));

    let (inspector, asset_id) = inspector_for(
        b"not a pdf",
        "application/pdf",
        PdfInspectionLimits::default(),
    );
    assert!(matches!(
        inspector.inspect(&asset_id).await,
        Err(PdfError::InvalidPdf)
    ));

    let bytes = pdf_bytes(
        Dictionary::new(),
        vec![(10, page((2, 0), Some(box_object(0, 0, 10, 10)), None, None))],
        &[10],
        None,
    );
    let (inspector, asset_id) = inspector_for(
        &bytes,
        "application/pdf",
        PdfInspectionLimits::new(1, 4096, 1024),
    );
    assert!(matches!(
        inspector.inspect(&asset_id).await,
        Err(PdfError::AssetTooLarge { .. })
    ));
}

#[tokio::test]
async fn zero_byte_asset_is_not_missing_but_is_not_a_pdf() {
    let (inspector, asset_id) =
        inspector_for(&[], "application/pdf", PdfInspectionLimits::default());
    assert!(matches!(
        inspector.inspect(&asset_id).await,
        Err(PdfError::InvalidPdf)
    ));
}

#[tokio::test]
async fn encrypted_pdf_is_rejected_before_parser_output() {
    let mut document = Document::with_version("1.7");
    document.max_id = 10;
    document.trailer.set(
        "ID",
        Object::Array(vec![
            Object::String(vec![1; 16], StringFormat::Literal),
            Object::String(vec![2; 16], StringFormat::Literal),
        ]),
    );
    document.objects.insert(
        (1, 0),
        dictionary!("Type" => "Catalog", "Pages" => Object::Reference((2, 0))).into(),
    );
    document.objects.insert(
        (2, 0),
        dictionary!("Type" => "Pages", "Kids" => vec![Object::Reference((4, 0))], "Count" => 1)
            .into(),
    );
    document.objects.insert(
        (4, 0),
        dictionary!("Type" => "Page", "Parent" => Object::Reference((2, 0)), "MediaBox" => box_object(0, 0, 10, 10)).into(),
    );
    document.trailer.set("Root", Object::Reference((1, 0)));
    let version = EncryptionVersion::V2 {
        document: &document,
        owner_password: "owner",
        user_password: "password",
        key_length: 128,
        permissions: lopdf::Permissions::all(),
    };
    let state = EncryptionState::try_from(version).expect("encryption state should build");
    document.encrypt(&state).expect("fixture should encrypt");
    let mut bytes = Vec::new();
    document
        .save_to(&mut bytes)
        .expect("encrypted fixture should save");

    let (inspector, asset_id) =
        inspector_for(&bytes, "application/pdf", PdfInspectionLimits::default());
    assert!(matches!(
        inspector.inspect(&asset_id).await,
        Err(PdfError::EncryptedUnsupported)
    ));
}

struct FailingReader;

#[async_trait]
impl AssetReader for FailingReader {
    async fn read_range(
        &self,
        asset_id: &AssetId,
        _range: AssetByteRange,
    ) -> Result<AssetByteChunk, orbitrelay_asset_runtime::AssetReadError> {
        Err(orbitrelay_asset_runtime::AssetReadError::Unavailable {
            detail: format!("reader unavailable for {asset_id}"),
        })
    }
}

#[tokio::test]
async fn catalog_and_reader_failures_are_mapped_at_pdf_boundary() {
    let bytes = pdf_bytes(
        Dictionary::new(),
        vec![(10, page((2, 0), Some(box_object(0, 0, 10, 10)), None, None))],
        &[10],
        None,
    );
    let store = MemoryAssetStore::new();
    let asset_id = AssetId::new();
    store
        .insert_verified(
            descriptor(asset_id.clone(), &bytes, "application/pdf"),
            Bytes::copy_from_slice(&bytes),
        )
        .unwrap();
    let catalog = Arc::new(store);
    let inspector = PdfInspector::new(
        catalog,
        Arc::new(FailingReader),
        PdfInspectionLimits::default(),
    );
    assert!(matches!(
        inspector.inspect(&asset_id).await,
        Err(PdfError::ReadFailed)
    ));
}
