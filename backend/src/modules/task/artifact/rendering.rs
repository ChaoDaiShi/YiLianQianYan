use super::{ArtifactGenerationError, ArtifactSource};
use crate::modules::task::deliverable_renderers::{
    self, ChartContent, DeliverableFormat, DocumentContent, RenderError, RenderedDeliverable,
    SheetContent,
};
use crate::modules::task::model::ArtifactType;

pub(super) fn render_source_output(
    file_name: &str,
    format: DeliverableFormat,
    source: &ArtifactSource,
    output: Option<&serde_json::Value>,
    source_text: &str,
) -> Result<RenderedDeliverable, ArtifactGenerationError> {
    let structured = output
        .and_then(|value| value.get("structured"))
        .filter(|value| !value.is_null());
    match format {
        DeliverableFormat::Markdown => {
            if matches!(source, ArtifactSource::Task { .. }) {
                return Ok(deliverable_renderers::render_markdown(
                    file_name,
                    source_text.into(),
                ));
            }
            if let Some(document) =
                structured.and_then(|value| value.get("report").or_else(|| value.get("document")))
            {
                if let Some(text) = document.as_str() {
                    return Ok(deliverable_renderers::render_markdown(
                        file_name,
                        text.into(),
                    ));
                }
                if let Ok(document) = serde_json::from_value::<DocumentContent>(document.clone()) {
                    return Ok(deliverable_renderers::render_document_markdown(
                        file_name, &document,
                    ));
                }
                let text = serde_json::to_string_pretty(document)
                    .map_err(|error| RenderError::Encode(error.to_string()))?;
                return Ok(deliverable_renderers::render_markdown(file_name, text));
            }
            if let Some(text) = output
                .and_then(|value| {
                    value
                        .get("content")
                        .or_else(|| value.get("result"))
                        .or_else(|| value.get("summary"))
                })
                .and_then(serde_json::Value::as_str)
                .filter(|text| !text.trim().is_empty())
            {
                return Ok(deliverable_renderers::render_markdown(
                    file_name,
                    text.into(),
                ));
            }
            if let Some(text) = structured.and_then(serde_json::Value::as_str) {
                return Ok(deliverable_renderers::render_markdown(
                    file_name,
                    text.into(),
                ));
            }
            Err(ArtifactGenerationError::StructuredPayloadMissing)
        }
        DeliverableFormat::Json => {
            let value = structured
                .or_else(|| {
                    matches!(source, ArtifactSource::Task { .. })
                        .then_some(output)
                        .flatten()
                })
                .ok_or(ArtifactGenerationError::StructuredPayloadMissing)?;
            Ok(deliverable_renderers::render_json(file_name, value)?)
        }
        DeliverableFormat::Docx | DeliverableFormat::Pptx | DeliverableFormat::Pdf => {
            let document = structured
                .and_then(|value| value.get("report").or_else(|| value.get("document")))
                .cloned()
                .ok_or(ArtifactGenerationError::StructuredPayloadMissing)?;
            let document: DocumentContent = serde_json::from_value(document)
                .map_err(|error| RenderError::InvalidPayload(error.to_string()))?;
            if format == DeliverableFormat::Docx {
                Ok(deliverable_renderers::render_docx(file_name, &document)?)
            } else if format == DeliverableFormat::Pptx {
                Ok(deliverable_renderers::render_pptx(file_name, &document)?)
            } else {
                Ok(deliverable_renderers::render_pdf(file_name, &document)?)
            }
        }
        DeliverableFormat::Xlsx => {
            let sheet = structured
                .and_then(|value| value.get("sheet"))
                .cloned()
                .ok_or(ArtifactGenerationError::StructuredPayloadMissing)?;
            let sheet: SheetContent = serde_json::from_value(sheet)
                .map_err(|error| RenderError::InvalidPayload(error.to_string()))?;
            Ok(deliverable_renderers::render_xlsx(file_name, &sheet)?)
        }
        DeliverableFormat::Png => {
            let chart = structured
                .and_then(|value| value.get("chart"))
                .cloned()
                .ok_or(ArtifactGenerationError::StructuredPayloadMissing)?;
            let chart: ChartContent = serde_json::from_value(chart)
                .map_err(|error| RenderError::InvalidPayload(error.to_string()))?;
            Ok(deliverable_renderers::render_png(file_name, &chart)?)
        }
    }
}

pub(super) fn artifact_type_for_mime(mime_type: &str) -> ArtifactType {
    match mime_type {
        "text/markdown" => ArtifactType::Report,
        "application/json"
        | "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => ArtifactType::Data,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        | "application/vnd.openxmlformats-officedocument.presentationml.presentation" => {
            ArtifactType::Document
        }
        "image/png" => ArtifactType::Image,
        "application/pdf" => ArtifactType::Document,
        _ => ArtifactType::Other,
    }
}
