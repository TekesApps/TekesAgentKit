//! Captured inline-image input and pre-send policy validation.
//!
//! The host supplies immutable bytes; this module detects their actual format,
//! validates the active endpoint policy, and commits them to the original
//! Composer context with its existing context/epoch compare-and-swap.

use crate::{PaneAccess, RunBinding};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tekes_agent_contract::{AttachmentPolicy, Result, require, unsupported};
use tekes_agent_presentation::{
    AttachmentFacts, ComposerArtifact, ComposerContextBaseline, ComposerDestination,
    PreparedSubmission, RunScope, SubmissionPlan,
};

const MAX_IMAGE_DECODE_ALLOCATION: u64 = 512 * 1024 * 1024;

/// Immutable bytes selected by the host. The supplied name is a basename;
/// MIME type and dimensions are always derived from the encoded image itself.
#[derive(Clone)]
pub struct InlineImageInput {
    basename: String,
    bytes: Arc<[u8]>,
}

impl std::fmt::Debug for InlineImageInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InlineImageInput")
            .field("basename", &self.basename)
            .field("byte_count", &self.bytes.len())
            .finish()
    }
}

impl InlineImageInput {
    pub fn new(basename: String, bytes: Arc<[u8]>) -> Result<Self> {
        require(
            !basename.trim().is_empty()
                && basename != "."
                && basename != ".."
                && !basename
                    .chars()
                    .any(|c| c.is_control() || c == '/' || c == '\\'),
            "inline image requires a basename",
        )?;
        require(!bytes.is_empty(), "empty inline image")?;
        Ok(Self { basename, bytes })
    }

    pub fn basename(&self) -> &str {
        &self.basename
    }

    pub fn byte_count(&self) -> u64 {
        self.bytes.len() as u64
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineImageOutcome {
    Attached,
    /// The original pane, epoch, or Composer context no longer accepts the
    /// captured selection. Nothing was committed to a newer context.
    Detached,
}

/// One immutable picker/clipboard capture. It has exactly one attach-or-cancel
/// claim and never performs an upload, submission or automatic retry.
pub struct InlineImageSelection {
    binding: Arc<RunBinding>,
    baseline: ComposerContextBaseline,
    epoch: u64,
    request_id: String,
    claimed: AtomicBool,
}

impl RunBinding {
    /// Image admission is a separate endpoint capability from generic-file
    /// upload; it requires only the shared attachment policy read.
    pub fn supports_inline_images(&self) -> bool {
        self.access() == PaneAccess::Ready
            && self
                .current_run()
                .is_some_and(|snapshot| snapshot.can_submit)
            && self.runtime.inspect(|state| {
                state.connection == tekes_agent_session::ConnectionState::Ready
                    && state
                        .endpoint_capabilities
                        .methods
                        .contains("attachments.policy")
            })
    }

    /// Capture the original writable Composer before opening a picker or
    /// starting another asynchronous host operation.
    pub fn begin_inline_images(
        self: &Arc<Self>,
        destination: &ComposerDestination,
    ) -> Result<InlineImageSelection> {
        let epoch = self.runtime.inspect(|state| state.connection_epoch);
        self.require_controls(destination)?;
        require(
            self.supports_inline_images(),
            "inline image policy unavailable",
        )?;
        let snapshot = self
            .current_run()
            .ok_or_else(|| unsupported("pane retired while selecting images"))?;
        let baseline = ComposerContextBaseline::read(&snapshot)
            .ok_or_else(|| unsupported("composer selection missing"))?;
        require(
            &baseline.selection.destination == destination,
            "inline image destination mismatch",
        )?;
        require(
            self.runtime.inspect(|state| state.connection_epoch) == epoch,
            "inline image connection changed",
        )?;
        ensure_command_accepts_attachments(&baseline.artifacts)?;
        Ok(InlineImageSelection {
            binding: self.clone(),
            baseline,
            epoch,
            request_id: uuid::Uuid::new_v4().to_string(),
            claimed: AtomicBool::new(false),
        })
    }

    /// Recheck policy for an already-frozen ticket after host preparation.
    /// Plain text and generic-file-only submissions keep their existing path.
    pub(crate) async fn validate_inline_submission(
        &self,
        ticket: &PreparedSubmission,
        scope: &RunScope,
        epoch: u64,
    ) -> Result<()> {
        require(scope == &self.scope, "submission scope changed")?;
        require(
            ticket.destination() == &ComposerDestination::run(scope.clone()),
            "submission destination changed",
        )?;
        let blocks = submission_attachments(ticket.plan());
        if !blocks.iter().any(|block| block["type"] == "image") {
            return Ok(());
        }

        self.require_controls(ticket.destination())?;
        require(
            self.runtime.inspect(|state| state.connection_epoch) == epoch,
            "submission connection changed",
        )?;
        let policy = self
            .runtime
            .attachment_policy(Some(&scope.session_id))
            .await;
        self.require_controls(ticket.destination())?;
        require(
            self.runtime.inspect(|state| state.connection_epoch) == epoch,
            "submission connection changed",
        )?;
        let policy = policy?;

        let attachment_facts = ticket.attachment_facts();
        require(
            attachment_facts.len() == blocks.len(),
            "prepared attachment facts do not match submission plan",
        )?;
        let mut sizes = Vec::with_capacity(blocks.len());
        let mut image_data = Vec::new();
        for (index, block) in blocks.iter().enumerate() {
            let frozen = &attachment_facts[index];
            require(
                !frozen.artifact_id().is_empty(),
                "prepared attachment identity missing",
            )?;
            let facts = frozen.facts();
            let size = match block["type"].as_str() {
                Some("image") => {
                    let encoded = block["data"]
                        .as_str()
                        .ok_or_else(|| unsupported("prepared image data missing"))?;
                    let size = encoded_image_size(encoded)?;
                    if let Some(frozen) = facts {
                        require(
                            frozen.byte_count == size,
                            "prepared image facts do not match image bytes",
                        )?;
                    }
                    image_data.push(encoded);
                    Some(size)
                }
                Some("file") => facts.map(|facts| facts.byte_count),
                _ => return Err(unsupported("unknown prepared attachment type")),
            };
            sizes.push(size);
        }
        validate_attachment_sizes(&policy, &sizes)?;

        let decode_policy = DecodePolicy::from_attachment_policy(&policy);
        // The ticket is borrowed across preparation, so the blocking decoder
        // needs owned encoded input. Defer this one payload copy until byte
        // and mixed-message limits have accepted the frozen ticket.
        let image_data = image_data
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let images = tokio::task::spawn_blocking(move || {
            image_data
                .iter()
                .map(|encoded| {
                    let bytes = STANDARD
                        .decode(encoded)
                        .map_err(|_| unsupported("invalid prepared image encoding"))?;
                    let decoded = decode_image(&bytes, &decode_policy)?;
                    Ok((bytes.len() as u64, decoded))
                })
                .collect::<Result<Vec<_>>>()
        })
        .await
        .map_err(|_| unsupported("inline image validation worker failed"));

        self.require_controls(ticket.destination())?;
        require(
            self.runtime.inspect(|state| state.connection_epoch) == epoch,
            "submission connection changed",
        )?;
        let images = images??;
        let mut image_index = 0;
        for (index, block) in blocks.iter().enumerate() {
            if block["type"] != "image" {
                continue;
            }
            let (actual_size, decoded) = &images[image_index];
            image_index += 1;
            require(
                block["data"].as_str().is_some()
                    && *actual_size > 0
                    && sizes[index] == Some(*actual_size),
                "invalid prepared image bytes",
            )?;
            require(
                policy
                    .inline_media_types
                    .iter()
                    .any(|allowed| allowed == &decoded.media_type),
                "inline image media type unsupported",
            )?;
            let declared = block["mediaType"]
                .as_str()
                .or_else(|| block["mimeType"].as_str());
            require(
                declared == Some(decoded.media_type.as_str()),
                "prepared image media type does not match actual bytes",
            )?;
            if let Some(facts) = attachment_facts[index].facts() {
                require(
                    facts.byte_count == *actual_size
                        && facts
                            .media_type
                            .as_deref()
                            .is_none_or(|mime| mime == decoded.media_type),
                    "prepared image facts do not match decoded image",
                )?;
            }
        }
        Ok(())
    }
}

impl InlineImageSelection {
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn destination(&self) -> &ComposerDestination {
        &self.baseline.selection.destination
    }

    /// Cancel the captured selection once, before any context mutation.
    pub fn cancel_selection(&self) -> bool {
        !self.claimed.swap(true, Ordering::AcqRel)
    }

    fn current(&self) -> bool {
        if self.binding.require_controls(self.destination()).is_err()
            || !self.binding.supports_inline_images()
            || self.binding.runtime.inspect(|state| state.connection_epoch) != self.epoch
        {
            return false;
        }
        self.binding
            .current_run()
            .is_some_and(|snapshot| self.baseline.matches_snapshot(&snapshot))
    }

    /// Validate every selected image off the host event thread, then append the
    /// complete batch through the original context-at-epoch compare-and-swap.
    pub async fn attach(&self, inputs: &[InlineImageInput]) -> Result<InlineImageOutcome> {
        require(
            !self.claimed.swap(true, Ordering::AcqRel),
            "inline image selection already attempted",
        )?;
        require(!inputs.is_empty(), "no selected images")?;
        ensure_command_accepts_attachments(&self.baseline.artifacts)?;
        if !self.current() {
            return Ok(InlineImageOutcome::Detached);
        }

        let policy = self
            .binding
            .runtime
            .attachment_policy(Some(&self.binding.scope.session_id))
            .await;
        if !self.current() {
            return Ok(InlineImageOutcome::Detached);
        }
        let policy = policy?;
        let sizes = inputs
            .iter()
            .map(InlineImageInput::byte_count)
            .collect::<Vec<_>>();
        validate_capture_policy(&policy, &self.baseline.artifacts, &sizes)?;

        let requests = inputs
            .iter()
            .map(|input| ImageDecodeRequest {
                basename: input.basename.clone(),
                bytes: input.bytes.clone(),
            })
            .collect::<Vec<_>>();
        let decode_policy = DecodePolicy::from_attachment_policy(&policy);
        let images = tokio::task::spawn_blocking(move || {
            requests
                .into_iter()
                .map(|request| {
                    let decoded = decode_image(&request.bytes, &decode_policy)?;
                    let media_type = decoded.media_type;
                    let data = STANDARD.encode(request.bytes.as_ref());
                    Ok(ValidatedImage {
                        basename: request.basename,
                        byte_count: request.bytes.len() as u64,
                        media_type,
                        data,
                    })
                })
                .collect::<Result<Vec<_>>>()
        })
        .await
        .map_err(|_| unsupported("inline image validation worker failed"));

        if !self.current() {
            return Ok(InlineImageOutcome::Detached);
        }
        let images = images??;
        for image in &images {
            require(
                policy
                    .inline_media_types
                    .iter()
                    .any(|allowed| allowed == &image.media_type),
                "inline image media type unsupported",
            )?;
        }

        let mut artifacts = self.baseline.artifacts.clone();
        for (index, image) in images.into_iter().enumerate() {
            artifacts.push(ComposerArtifact::Attachment {
                id: format!("image:{}:{index}", self.request_id),
                name: image.basename.clone(),
                content: json!({
                    "type": "image",
                    "mediaType": image.media_type,
                    "data": image.data,
                    "name": image.basename,
                }),
                facts: Some(AttachmentFacts {
                    attachment_id: None,
                    byte_count: image.byte_count,
                    media_type: Some(image.media_type),
                }),
            });
        }
        if !self.current() {
            return Ok(InlineImageOutcome::Detached);
        }
        let committed = self
            .binding
            .controller
            .as_ref()
            .expect("captured writable controller")
            .patch_context_at_epoch(
                &self.baseline,
                artifacts,
                self.baseline.annotations.clone(),
                self.epoch,
            )?;
        Ok(if committed {
            InlineImageOutcome::Attached
        } else {
            InlineImageOutcome::Detached
        })
    }
}

struct ImageDecodeRequest {
    basename: String,
    bytes: Arc<[u8]>,
}

struct ValidatedImage {
    basename: String,
    byte_count: u64,
    media_type: String,
    data: String,
}

struct DecodePolicy {
    inline_media_types: Vec<String>,
    max_image_pixels: Option<u64>,
    max_image_dimension: Option<u64>,
}

impl DecodePolicy {
    fn from_attachment_policy(policy: &AttachmentPolicy) -> Self {
        Self {
            inline_media_types: policy.inline_media_types.clone(),
            max_image_pixels: policy.max_image_pixels,
            max_image_dimension: policy.max_image_dimension,
        }
    }
}

#[derive(Debug)]
struct DecodedImage {
    media_type: String,
}

fn decode_image(bytes: &[u8], policy: &DecodePolicy) -> Result<DecodedImage> {
    require(!bytes.is_empty(), "empty inline image")?;
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| unsupported("invalid inline image data"))?;
    let format = reader
        .format()
        .ok_or_else(|| unsupported("inline image format missing"))?;
    let media_type = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::Gif => "image/gif",
        image::ImageFormat::WebP => "image/webp",
        _ => return Err(unsupported("inline image format unsupported")),
    }
    .to_owned();
    require(
        policy
            .inline_media_types
            .iter()
            .any(|mime| mime == &media_type),
        "inline image media type unsupported",
    )?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|_| unsupported("invalid inline image dimensions"))?;
    require(width > 0 && height > 0, "invalid inline image dimensions")?;
    if let Some(limit) = policy.max_image_dimension {
        require(
            u64::from(width) <= limit && u64::from(height) <= limit,
            "inline image dimension limit",
        )?;
    }
    let pixels = u64::from(width) * u64::from(height);
    if let Some(limit) = policy.max_image_pixels {
        require(pixels <= limit, "inline image pixel limit")?;
    }

    // Image::Limits bounds decoder working memory; max_alloc is best-effort for
    // some codecs. Protocol dimension/pixel limits above are checked separately.
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_IMAGE_DECODE_ALLOCATION);
    if let Some(limit) = policy.max_image_dimension {
        let limit = u32::try_from(limit).unwrap_or(u32::MAX);
        limits.max_image_width = Some(limit);
        limits.max_image_height = Some(limit);
    }
    let mut decoder = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| unsupported("invalid inline image data"))?;
    decoder.limits(limits);
    decoder
        .decode()
        .map_err(|_| unsupported("invalid inline image pixels"))?;
    Ok(DecodedImage { media_type })
}

fn ensure_command_accepts_attachments(artifacts: &[ComposerArtifact]) -> Result<()> {
    for artifact in artifacts {
        if let ComposerArtifact::Command {
            supports_attachments,
            ..
        } = artifact
        {
            require(*supports_attachments, "command does not accept attachments")?;
        }
    }
    Ok(())
}

fn validate_capture_policy(
    policy: &AttachmentPolicy,
    existing: &[ComposerArtifact],
    new_sizes: &[u64],
) -> Result<()> {
    require(!new_sizes.is_empty(), "no selected images")?;
    let mut sizes = Vec::new();
    for artifact in existing {
        let ComposerArtifact::Attachment { content, facts, .. } = artifact else {
            continue;
        };
        let size = match content["type"].as_str() {
            Some("image") => {
                let data = content["data"]
                    .as_str()
                    .ok_or_else(|| unsupported("existing image data missing"))?;
                let size = encoded_image_size(data)?;
                if let Some(facts) = facts {
                    require(
                        facts.byte_count == size,
                        "existing image facts do not match bytes",
                    )?;
                }
                Some(size)
            }
            Some("file") => facts.as_ref().map(|facts| facts.byte_count),
            _ => return Err(unsupported("unknown existing attachment type")),
        };
        sizes.push(size);
    }
    sizes.extend(new_sizes.iter().copied().map(Some));
    validate_attachment_sizes(policy, &sizes)
}

fn validate_count(policy: &AttachmentPolicy, existing: usize, new: usize) -> Result<()> {
    require(
        policy
            .max_attachments_per_message
            .is_none_or(|limit| (existing as u64).saturating_add(new as u64) <= limit),
        "attachment count limit",
    )
}

fn submission_attachments(plan: &SubmissionPlan) -> Vec<&Value> {
    match plan {
        SubmissionPlan::Prompt { content, .. } => content
            .iter()
            .filter(|part| matches!(part["type"].as_str(), Some("file" | "image")))
            .collect(),
        SubmissionPlan::Command { attachments, .. } => attachments
            .iter()
            .filter(|part| matches!(part["type"].as_str(), Some("file" | "image")))
            .collect(),
    }
}

fn encoded_image_size(encoded: &str) -> Result<u64> {
    require(
        !encoded.is_empty() && encoded.len().is_multiple_of(4),
        "invalid prepared image encoding",
    )?;
    let padding = encoded
        .as_bytes()
        .iter()
        .rev()
        .take_while(|byte| **byte == b'=')
        .count();
    require(padding <= 2, "invalid prepared image padding")?;
    let decoded = encoded
        .len()
        .checked_div(4)
        .and_then(|groups| groups.checked_mul(3))
        .and_then(|size| size.checked_sub(padding))
        .ok_or_else(|| unsupported("prepared image size overflow"))?;
    u64::try_from(decoded).map_err(|_| unsupported("prepared image size overflow"))
}

fn validate_attachment_sizes(policy: &AttachmentPolicy, sizes: &[Option<u64>]) -> Result<()> {
    validate_count(policy, 0, sizes.len())?;
    if let Some(limit) = policy.max_attachment_bytes {
        for size in sizes {
            require(
                size.is_some_and(|size| size <= limit),
                "attachment size limit or unknown size",
            )?;
        }
    }
    if let Some(limit) = policy.max_message_attachment_bytes {
        let total = sizes.iter().try_fold(0u64, |total, size| {
            total
                .checked_add(
                    (*size).ok_or_else(|| unsupported("prepared attachment byte count unknown"))?,
                )
                .ok_or_else(|| unsupported("attachment byte count overflow"))
        })?;
        require(total <= limit, "message attachment size limit")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> AttachmentPolicy {
        serde_json::from_value(json!({"inlineMediaTypes":["image/png","image/jpeg","image/gif","image/webp"],
            "maxAttachmentBytes":4194304,"maxAttachmentsPerMessage":8,"maxMessageAttachmentBytes":16777216,
            "maxImagePixels":33554432,"maxImageDimension":8192})).unwrap()
    }
    fn encoded(format: image::ImageFormat, width: u32, height: u32) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(width, height)
            .write_to(&mut bytes, format)
            .unwrap();
        bytes.into_inner()
    }
    #[test]
    fn supported_formats_are_detected_from_actual_bytes_and_fully_decoded() {
        for (format, mime) in [
            (image::ImageFormat::Png, "image/png"),
            (image::ImageFormat::Jpeg, "image/jpeg"),
            (image::ImageFormat::Gif, "image/gif"),
            (image::ImageFormat::WebP, "image/webp"),
        ] {
            let bytes = encoded(format, 2, 3);
            assert_eq!(
                decode_image(&bytes, &DecodePolicy::from_attachment_policy(&policy()))
                    .unwrap()
                    .media_type,
                mime
            );
        }
    }
    #[test]
    fn policy_checks_mime_before_pixels_and_rejects_corrupt_payloads_and_dimensions() {
        let mut bytes = encoded(image::ImageFormat::Png, 8, 1);
        let idat = bytes.windows(4).position(|part| part == b"IDAT").unwrap();
        bytes[idat + 5] ^= 0xff;
        let mut denied = policy();
        denied.inline_media_types.clear();
        assert!(
            decode_image(&bytes, &DecodePolicy::from_attachment_policy(&denied))
                .unwrap_err()
                .to_string()
                .contains("media type")
        );
        assert!(
            decode_image(&bytes, &DecodePolicy::from_attachment_policy(&policy()))
                .unwrap_err()
                .to_string()
                .contains("pixels")
        );
        let valid = encoded(image::ImageFormat::Png, 8, 1);
        let mut dimension = policy();
        dimension.max_image_dimension = Some(7);
        assert!(
            decode_image(&valid, &DecodePolicy::from_attachment_policy(&dimension))
                .unwrap_err()
                .to_string()
                .contains("dimension limit")
        );
        let mut pixels = policy();
        pixels.max_image_pixels = Some(7);
        assert!(
            decode_image(&valid, &DecodePolicy::from_attachment_policy(&pixels))
                .unwrap_err()
                .to_string()
                .contains("pixel limit")
        );
    }
    #[test]
    fn adding_images_uses_existing_encoded_bytes_even_without_legacy_image_facts() {
        let bytes = encoded(image::ImageFormat::Png, 1, 1);
        let size = bytes.len() as u64;
        let mut existing = ComposerArtifact::Attachment {
            id: "legacy-image".into(),
            name: "旧图.png".into(),
            content: json!({"type":"image","mediaType":"image/png","data":STANDARD.encode(&bytes)}),
            facts: None,
        };
        let mut limited = policy();
        limited.max_message_attachment_bytes = Some(size * 2);
        assert!(validate_capture_policy(&limited, &[existing.clone()], &[size]).is_ok());
        limited.max_message_attachment_bytes = Some(size * 2 - 1);
        assert!(validate_capture_policy(&limited, &[existing.clone()], &[size]).is_err());
        if let ComposerArtifact::Attachment { facts, .. } = &mut existing {
            *facts = Some(AttachmentFacts {
                attachment_id: None,
                byte_count: 0,
                media_type: Some("image/png".into()),
            });
        }
        assert!(validate_capture_policy(&policy(), &[existing], &[size]).is_err());
    }

    #[test]
    fn mixed_size_limits_reject_unknown_legacy_facts_and_checked_sum_overflow() {
        let mut limited = policy();
        limited.max_message_attachment_bytes = Some(78);
        assert!(validate_attachment_sizes(&limited, &[Some(69), Some(10)]).is_err());
        assert!(validate_attachment_sizes(&limited, &[None, Some(69)]).is_err());
        limited.max_message_attachment_bytes = Some(79);
        assert!(validate_attachment_sizes(&limited, &[Some(69), Some(10)]).is_ok());
        limited.max_attachment_bytes = None;
        limited.max_message_attachment_bytes = Some(u64::MAX);
        assert!(validate_attachment_sizes(&limited, &[Some(u64::MAX), Some(1)]).is_err());
        limited.max_message_attachment_bytes = None;
        assert!(validate_attachment_sizes(&limited, &[None, Some(69)]).is_ok());
    }
    #[test]
    fn base64_size_preflight_handles_padding_and_rejects_invalid_shapes() {
        for bytes in [vec![0], vec![0, 1], vec![0, 1, 2], vec![0, 1, 2, 3]] {
            assert_eq!(
                encoded_image_size(&STANDARD.encode(&bytes)).unwrap(),
                bytes.len() as u64
            );
        }
        for bad in ["", "a", "abcd=", "a===", "===="] {
            assert!(encoded_image_size(bad).is_err());
        }
    }
}
