use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};

const MAX_INLINE_IMAGE_BYTES: usize = 4 * 1024 * 1024;
const MAX_INLINE_IMAGE_DIMENSION: u32 = 8192;
const MAX_INLINE_IMAGE_PIXELS: u64 = 33_554_432;
// This is a guard for decoder working memory. Pixel and dimension checks below remain the
// protocol limits; image::Limits::max_alloc is best-effort for some supported codecs.
const MAX_INLINE_IMAGE_DECODE_ALLOCATION: u64 = 256 * 1024 * 1024;
const MAX_UPLOAD_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_PROMPT_ATTACHMENT_BYTES: u64 = 16 * 1024 * 1024;

/// Provider-independent attachment facts collected from this session's verified upload records.
/// These facts intentionally omit local paths and do not alter the native Pi prompt payload.
pub(crate) struct MaterializedPrompt {
    pub(crate) message: String,
    pub(crate) images: Vec<Value>,
    pub(crate) files: Vec<Value>,
}

fn check_image_dimensions(width: u32, height: u32) -> Result<()> {
    require(
        width > 0
            && height > 0
            && width <= MAX_INLINE_IMAGE_DIMENSION
            && height <= MAX_INLINE_IMAGE_DIMENSION,
        "image dimension limit",
    )?;
    require(
        u64::from(width) * u64::from(height) <= MAX_INLINE_IMAGE_PIXELS,
        "image pixel limit",
    )
}

fn validate_inline_image(bytes: &[u8]) -> Result<(u32, u32)> {
    require(
        !bytes.is_empty() && bytes.len() <= MAX_INLINE_IMAGE_BYTES,
        "invalid image or image limit",
    )?;

    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| unsupported("invalid image data"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_INLINE_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_INLINE_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_INLINE_IMAGE_DECODE_ALLOCATION);
    reader.limits(limits);
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| unsupported("invalid image dimensions"))?;
    check_image_dimensions(width, height)?;

    // Reading dimensions alone accepts truncated or corrupt pixel payloads. Decode the first
    // frame with the same resource bounds before retaining the original encoded bytes.
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| unsupported("invalid image data"))?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_INLINE_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_INLINE_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_INLINE_IMAGE_DECODE_ALLOCATION);
    reader.limits(limits);
    reader
        .decode()
        .map_err(|_| unsupported("invalid image pixels"))?;
    Ok((width, height))
}

fn inline_image_policy() -> Value {
    json!({
        "inlineMediaTypes": ["image/png", "image/jpeg", "image/gif", "image/webp"],
        "maxAttachmentBytes": MAX_INLINE_IMAGE_BYTES,
        "maxAttachmentsPerMessage": 8,
        "maxMessageAttachmentBytes": 16 * 1024 * 1024,
        "maxImagePixels": MAX_INLINE_IMAGE_PIXELS,
        "maxImageDimension": MAX_INLINE_IMAGE_DIMENSION,
    })
}

impl Context {
    pub(crate) fn upload_directory(&self, id: &str) -> PathBuf {
        self.config
            .data_root
            .join("uploads")
            .join(format!("{:x}", Sha256::digest(id.as_bytes())))
    }
    async fn upload_record(&self, id: &str, attachment_id: &str) -> Result<(Value, PathBuf)> {
        self.row(id).await?;
        require(
            uuid::Uuid::parse_str(attachment_id).is_ok(),
            "invalid attachment identity",
        )?;
        let path = self
            .upload_directory(id)
            .join(format!("{attachment_id}.json"));
        let bytes = tokio::fs::read(path).await.map_err(io_error)?;
        let record: Value = serde_json::from_slice(&bytes)?;
        require(
            record["attachmentId"] == attachment_id,
            "attachment identity mismatch",
        )?;
        let file = self
            .upload_directory(id)
            .join(format!("{attachment_id}.content"));
        require(
            tokio::fs::metadata(&file).await.map_err(io_error)?.len()
                == record["bytes"].as_u64().unwrap_or(u64::MAX),
            "attachment size changed",
        )?;
        Ok((record, file))
    }

    pub(crate) async fn materialize(
        &self,
        id: &str,
        content: &[Value],
    ) -> Result<MaterializedPrompt> {
        let mut texts = Vec::new();
        let mut images = Vec::new();
        let mut files = Vec::new();
        let mut total = 0u64;
        let mut attachments = 0;
        require(!content.is_empty(), "empty prompt")?;
        for (content_index, part) in content.iter().enumerate() {
            match string(part, "type")? {
                "text" => texts.push(string(part, "text")?.to_owned()),
                "image" => {
                    let data = string(part, "data")?;
                    let bytes = STANDARD
                        .decode(data)
                        .map_err(|_| Error::Protocol("invalid image encoding".into()))?;
                    let mime = part["mediaType"]
                        .as_str()
                        .or_else(|| part["mimeType"].as_str())
                        .ok_or_else(|| unsupported("image media type required"))?;
                    require(
                        ["image/png", "image/jpeg", "image/gif", "image/webp"].contains(&mime)
                            && !bytes.is_empty()
                            && bytes.len() <= MAX_INLINE_IMAGE_BYTES,
                        "invalid image or image limit",
                    )?;
                    let (width, height) = validate_inline_image(&bytes)?;
                    total += bytes.len() as u64;
                    attachments += 1;
                    let attachment_id = format!("{:x}", Sha256::digest(&bytes));
                    let dir = self.upload_directory(id);
                    tokio::fs::create_dir_all(&dir).await.map_err(io_error)?;
                    tokio::fs::write(dir.join(format!("image-{attachment_id}.json")),serde_json::to_vec(&json!({"attachment":{"attachmentId":attachment_id,"mediaType":mime,"bytes":bytes.len(),"width":width,"height":height,"name":part["name"]},"data":data}))?).await.map_err(io_error)?;
                    images.push(json!({"type":"image","mimeType":mime,"data":data}));
                }
                "file" => {
                    let receipt = string(part, "receiptId")?;
                    let (record, path) = self.upload_record(id, receipt).await?;
                    // upload_record already validated the stored length. Keep
                    // facts and budget tied to that exact receipt metadata.
                    let bytes = record["bytes"]
                        .as_u64()
                        .ok_or_else(|| Error::Protocol("invalid upload size".into()))?;
                    require(
                        bytes > 0 && bytes <= MAX_UPLOAD_FILE_BYTES,
                        "upload file limit",
                    )?;
                    files.push(json!({
                        "contentIndex": content_index,
                        "precedingImageCount": images.len(),
                        "file": {
                            "attachmentId": string(&record, "attachmentId")?,
                            "name": string(&record, "name")?,
                            "bytes": bytes
                        }
                    }));
                    // Preserve Pi's existing prompt text contract. The structured `files`
                    // projection above deliberately contains no host-local path.
                    texts.push(format!(
                        "Attached file: {} (path: {})",
                        record["name"],
                        serde_json::to_string(&path)?
                    ));
                    total += bytes;
                    attachments += 1;
                }
                _ => return Err(unsupported("unknown prompt content")),
            }
            require(
                attachments <= 8 && total <= MAX_PROMPT_ATTACHMENT_BYTES,
                "message attachment limit",
            )?;
        }
        let message = texts.join("\n\n");
        require(
            attachments > 0 || !message.trim().is_empty(),
            "empty prompt",
        )?;
        require(
            !message.trim_start().starts_with("/agentkit-control"),
            "reserved host command",
        )?;
        Ok(MaterializedPrompt {
            message,
            images,
            files,
        })
    }
}
impl PiSessionEndpoint {
    pub(crate) async fn attachment_call(
        &self,
        method: &str,
        payload: &Value,
    ) -> Option<Result<Value>> {
        match method {
            "attachments.policy"=>Some(Ok(inline_image_policy())),
            "session.uploadFile"=>Some(async {
                let id=string(payload,"sessionId")?;self.context.row(id).await?;
                let name=string(payload,"name")?;
                require(!name.is_empty() && !name.contains(['\0','/','\\']),"invalid upload name")?;
                let data=STANDARD.decode(string(payload,"data")?).map_err(|_|unsupported("invalid upload encoding"))?;
                require(!data.is_empty() && data.len()<=8*1024*1024,"upload limit")?;
                let receipt=uuid::Uuid::new_v4().to_string();let dir=self.context.upload_directory(id);
                tokio::fs::create_dir_all(&dir).await.map_err(io_error)?;
                let record=json!({"attachmentId":receipt,"name":name,"mediaType":payload["mediaType"].as_str().unwrap_or("application/octet-stream"),"bytes":data.len()});
                tokio::fs::write(dir.join(format!("{receipt}.content")),&data).await.map_err(io_error)?;
                tokio::fs::write(dir.join(format!("{receipt}.json")),serde_json::to_vec(&record)?).await.map_err(io_error)?;
                Ok(json!({"receiptId":receipt,"file":{"attachmentId":receipt,"name":name,"bytes":data.len()}}))
            }.await),
            "session.fileAttachment"=>Some(async {
                let (record,path)=self.context.upload_record(string(payload,"sessionId")?,string(payload,"attachmentId")?).await?;
                let bytes=tokio::fs::read(path).await.map_err(io_error)?;
                Ok(json!({"attachment":record,"data":STANDARD.encode(bytes)}))
            }.await),
            "session.attachment"=>Some(async {
                let id=string(payload,"sessionId")?;self.context.row(id).await?;
                let attachment=string(payload,"attachmentId")?;
                require(attachment.len()==64 && attachment.bytes().all(|c|c.is_ascii_hexdigit()),"invalid image identity")?;
                let bytes=tokio::fs::read(self.context.upload_directory(id).join(format!("image-{attachment}.json"))).await.map_err(io_error)?;
                Ok(serde_json::from_slice(&bytes)?)
            }.await),
            _=>None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn insert_session(context: &Context, id: &str) {
        context.manifest.lock().await.sessions.insert(
            id.into(),
            Row {
                id: id.into(),
                cwd: context.config.data_root.clone(),
                file: context.config.data_root.join("sessions/test.jsonl"),
                title: None,
                archived: false,
                parent: None,
                subagent_parent: None,
                model_selection: None,
                ephemeral: false,
                receipts: BTreeMap::new(),
                materialized: false,
                native_session_id: None,
                workspace_id: None,
                queue: Vec::new(),
                control_revision: 0,
                goal_revision: 0,
                goal: None,
                feedback: BTreeMap::new(),
            },
        );
    }

    async fn write_test_upload(context: &Context, session: &str, name: &str, bytes: u64) -> String {
        let attachment_id = uuid::Uuid::new_v4().to_string();
        let directory = context.upload_directory(session);
        tokio::fs::create_dir_all(&directory).await.unwrap();
        let content_path = directory.join(format!("{attachment_id}.content"));
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(content_path)
            .await
            .unwrap();
        file.set_len(bytes).await.unwrap();
        tokio::fs::write(
            directory.join(format!("{attachment_id}.json")),
            serde_json::to_vec(&json!({
                "attachmentId": attachment_id,
                "name": name,
                "bytes": bytes
            }))
            .unwrap(),
        )
        .await
        .unwrap();
        attachment_id
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::DynamicImage::new_rgb8(width, height);
        let mut bytes = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, image::ImageFormat::Png)
            .expect("encode test PNG");
        bytes.into_inner()
    }

    #[test]
    fn accepts_and_decodes_a_small_image() {
        for format in [
            image::ImageFormat::Png,
            image::ImageFormat::Jpeg,
            image::ImageFormat::Gif,
            image::ImageFormat::WebP,
        ] {
            let mut bytes = std::io::Cursor::new(Vec::new());
            image::DynamicImage::new_rgb8(2, 3)
                .write_to(&mut bytes, format)
                .unwrap();
            assert_eq!(
                validate_inline_image(bytes.get_ref()).unwrap(),
                (2, 3),
                "{format:?}"
            );
        }
    }

    #[test]
    fn rejects_a_valid_header_with_corrupt_pixel_payload() {
        let mut bytes = png(2, 3);
        let idat = bytes
            .windows(4)
            .position(|window| window == b"IDAT")
            .expect("PNG IDAT chunk");
        let payload_byte = idat + 5;
        bytes[payload_byte] ^= 0xff;
        let dimensions = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()
            .unwrap()
            .into_dimensions()
            .unwrap();
        assert_eq!(dimensions, (2, 3));
        assert!(validate_inline_image(&bytes).is_err());
    }

    #[test]
    fn enforces_clientkit_dimension_and_pixel_limits() {
        assert!(check_image_dimensions(8192, 4096).is_ok());
        assert!(check_image_dimensions(8193, 1).is_err());
        assert!(check_image_dimensions(8192, 8192).is_err());
        assert!(check_image_dimensions(0, 1).is_err());
    }

    #[test]
    fn advertises_the_same_image_policy_as_clientkit() {
        let policy = inline_image_policy();
        assert_eq!(policy["maxAttachmentBytes"], 4_194_304);
        assert_eq!(policy["maxAttachmentsPerMessage"], 8);
        assert_eq!(policy["maxMessageAttachmentBytes"], 16_777_216);
        assert_eq!(policy["maxImagePixels"], 33_554_432);
        assert_eq!(policy["maxImageDimension"], 8192);
        assert_eq!(
            policy["inlineMediaTypes"],
            json!(["image/png", "image/jpeg", "image/gif", "image/webp"])
        );
    }

    #[tokio::test]
    async fn materialize_preserves_original_image_and_rejects_corruption_before_storage() {
        let root = std::env::temp_dir().join(format!("agentkit-image-{}", uuid::Uuid::new_v4()));
        let connector = PiConnector::new(PiConfiguration::new(
            PathBuf::from("/bin/false"),
            root.clone(),
        ))
        .await
        .unwrap();
        let bytes = png(2, 3);
        let data = STANDARD.encode(&bytes);
        let input = json!({"type":"image","mediaType":"image/png","data":data,"name":"中文.png"});
        let materialized = connector
            .context
            .materialize("owned-image-session", &[input])
            .await
            .unwrap();
        assert!(materialized.message.is_empty());
        assert_eq!(
            materialized.images,
            vec![json!({"type":"image","mimeType":"image/png","data":data})]
        );
        let attachment_id = format!("{:x}", Sha256::digest(&bytes));
        let record: Value = serde_json::from_slice(
            &tokio::fs::read(
                connector
                    .context
                    .upload_directory("owned-image-session")
                    .join(format!("image-{attachment_id}.json")),
            )
            .await
            .unwrap(),
        )
        .unwrap();
        assert_eq!(record["data"], data);
        assert_eq!(record["attachment"]["attachmentId"], attachment_id);
        assert_eq!(record["attachment"]["bytes"], bytes.len());
        assert_eq!(record["attachment"]["name"], "中文.png");
        assert_eq!(record["attachment"]["width"], 2);
        assert_eq!(record["attachment"]["height"], 3);

        let mut corrupt = bytes;
        let idat = corrupt.windows(4).position(|w| w == b"IDAT").unwrap();
        corrupt[idat + 5] ^= 0xff;
        let result = connector
            .context
            .materialize(
                "rejected-image-session",
                &[json!({"type":"image","mediaType":"image/png","data":STANDARD.encode(corrupt)})],
            )
            .await;
        assert!(result.is_err());
        assert!(
            !connector
                .context
                .upload_directory("rejected-image-session")
                .exists()
        );
        drop(connector);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn mixed_content_preserves_file_positions_and_uses_session_receipts() {
        let root = std::env::temp_dir().join(format!("agentkit-mixed-{}", uuid::Uuid::new_v4()));
        let connector = PiConnector::new(PiConfiguration::new(
            PathBuf::from("/bin/false"),
            root.clone(),
        ))
        .await
        .unwrap();
        let session = "mixed-file-session";
        let other_session = "other-file-session";
        insert_session(&connector.context, session).await;
        insert_session(&connector.context, other_session).await;
        let first = write_test_upload(&connector.context, session, "一.txt", 7).await;
        let second = write_test_upload(&connector.context, session, "二.txt", 11).await;
        let foreign = write_test_upload(&connector.context, other_session, "foreign.txt", 5).await;
        let image_data = STANDARD.encode(png(1, 1));
        let mixed = connector
            .context
            .materialize(
                session,
                &[
                    json!({"type":"text","text":"before"}),
                    json!({"type":"file","receiptId":first}),
                    json!({"type":"image","mediaType":"image/png","data":image_data}),
                    json!({"type":"file","receiptId":second}),
                    json!({"type":"text","text":"after"}),
                    json!({"type":"image","mediaType":"image/png","data":image_data}),
                ],
            )
            .await
            .unwrap();
        assert_eq!(mixed.files.len(), 2);
        assert_eq!(mixed.files[0]["contentIndex"], 1);
        assert_eq!(mixed.files[0]["precedingImageCount"], 0);
        assert_eq!(mixed.files[0]["file"]["attachmentId"], first);
        assert_eq!(mixed.files[0]["file"]["name"], "一.txt");
        assert_eq!(mixed.files[0]["file"]["bytes"], 7);
        assert_eq!(mixed.files[1]["contentIndex"], 3);
        assert_eq!(mixed.files[1]["precedingImageCount"], 1);
        assert_eq!(mixed.files[1]["file"]["attachmentId"], second);
        assert_eq!(mixed.files[1]["file"]["bytes"], 11);
        assert_eq!(mixed.images.len(), 2);
        assert!(mixed.message.contains("Attached file: \"一.txt\" (path:"));
        assert!(
            !serde_json::to_string(&mixed.files)
                .unwrap()
                .contains("path")
        );

        let text_only = connector
            .context
            .materialize(session, &[json!({"type":"text","text":"plain prompt"})])
            .await
            .unwrap();
        assert_eq!(text_only.message, "plain prompt");
        assert!(text_only.images.is_empty());
        assert!(text_only.files.is_empty());

        assert!(
            connector
                .context
                .materialize(session, &[json!({"type":"file","receiptId":foreign})],)
                .await
                .is_err()
        );
        drop(connector);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn mixed_image_and_file_bytes_share_the_sixteen_mib_budget() {
        let root = std::env::temp_dir().join(format!("agentkit-budget-{}", uuid::Uuid::new_v4()));
        let connector = PiConnector::new(PiConfiguration::new(
            PathBuf::from("/bin/false"),
            root.clone(),
        ))
        .await
        .unwrap();
        let session = "attachment-budget-session";
        insert_session(&connector.context, session).await;
        let first = write_test_upload(
            &connector.context,
            session,
            "first.bin",
            MAX_UPLOAD_FILE_BYTES,
        )
        .await;
        let second = write_test_upload(
            &connector.context,
            session,
            "second.bin",
            MAX_UPLOAD_FILE_BYTES,
        )
        .await;
        let image_data = STANDARD.encode(png(1, 1));
        let result = connector
            .context
            .materialize(
                session,
                &[
                    json!({"type":"file","receiptId":first}),
                    json!({"type":"image","mediaType":"image/png","data":image_data}),
                    json!({"type":"file","receiptId":second}),
                ],
            )
            .await;
        assert!(result.is_err());
        drop(connector);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }
}
