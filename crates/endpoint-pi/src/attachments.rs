use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
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
    ) -> Result<(String, Vec<Value>)> {
        let mut texts = Vec::new();
        let mut images = Vec::new();
        let mut total = 0;
        let mut attachments = 0;
        require(!content.is_empty(), "empty prompt")?;
        for part in content {
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
                            && bytes.len() <= 4 * 1024 * 1024,
                        "invalid image or image limit",
                    )?;
                    let (width, height) = image::ImageReader::new(std::io::Cursor::new(&bytes))
                        .with_guessed_format()
                        .map_err(io_error)?
                        .into_dimensions()
                        .map_err(|_| unsupported("invalid image dimensions"))?;
                    require(
                        width > 0
                            && height > 0
                            && u64::from(width) * u64::from(height) <= 40_000_000,
                        "image pixel limit",
                    )?;
                    total += bytes.len();
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
                    texts.push(format!(
                        "Attached file: {} (path: {})",
                        record["name"],
                        serde_json::to_string(&path)?
                    ));
                    attachments += 1;
                }
                _ => return Err(unsupported("unknown prompt content")),
            }
            require(
                attachments <= 8 && total <= 16 * 1024 * 1024,
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
        Ok((message, images))
    }
}
impl PiSessionEndpoint {
    pub(crate) async fn attachment_call(
        &self,
        method: &str,
        payload: &Value,
    ) -> Option<Result<Value>> {
        match method {
            "attachments.policy"=>Some(Ok(json!({"inlineMediaTypes":["image/png","image/jpeg","image/gif","image/webp"],"maxAttachmentBytes":4194304,"maxAttachmentsPerMessage":8,"maxMessageAttachmentBytes":16777216,"maxImagePixels":40000000}))),
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
