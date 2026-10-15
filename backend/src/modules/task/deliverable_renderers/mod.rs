//! Real-byte renderers for explicitly requested task deliverable formats.

use serde::{Deserialize, Serialize};
use std::io::{Cursor, Write};

pub const DOCX_MIME: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
pub const XLSX_MIME: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
pub const PNG_WIDTH: u32 = 800;
pub const PNG_HEIGHT: u32 = 480;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedDeliverable {
    pub file_name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliverableFormat {
    Markdown,
    Json,
    Docx,
    Xlsx,
    Png,
    Pdf,
}

impl DeliverableFormat {
    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Markdown => "text/markdown",
            Self::Json => "application/json",
            Self::Docx => DOCX_MIME,
            Self::Xlsx => XLSX_MIME,
            Self::Png => "image/png",
            Self::Pdf => "application/pdf",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Json => "json",
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
            Self::Png => "png",
            Self::Pdf => "pdf",
        }
    }

    pub fn from_mime_type(value: &str) -> Result<Self, RenderError> {
        Self::try_from(value)
    }
}

impl TryFrom<&str> for DeliverableFormat {
    type Error = RenderError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "markdown" | "text/markdown" => Ok(Self::Markdown),
            "json" | "application/json" => Ok(Self::Json),
            "docx" | DOCX_MIME => Ok(Self::Docx),
            "xlsx" | XLSX_MIME => Ok(Self::Xlsx),
            "png" | "image/png" => Ok(Self::Png),
            "pdf" | "application/pdf" => Ok(Self::Pdf),
            other => Err(RenderError::UnsupportedFormat(other.to_owned())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentContent {
    pub title: String,
    #[serde(default)]
    pub sections: Vec<DocumentSection>,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentSection {
    pub heading: String,
    #[serde(default)]
    pub paragraphs: Vec<String>,
    #[serde(default)]
    pub table: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetContent {
    pub name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartContent {
    #[serde(default)]
    pub title: Option<String>,
    pub categories: Vec<String>,
    pub values: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    #[error("unsupported deliverable format: {0}")]
    UnsupportedFormat(String),
    #[error("invalid structured deliverable payload: {0}")]
    InvalidPayload(String),
    #[error("deliverable encoding failed: {0}")]
    Encode(String),
    #[error("pdf_renderer_integration_unavailable: {0}")]
    PdfRendererIntegrationUnavailable(String),
}

/// Produce a safe leaf filename with the extension matching the selected
/// format. A caller-supplied conflicting extension is rejected rather than
/// silently renamed to a different type.
pub fn file_name_for_format(name: &str, format: DeliverableFormat) -> Result<String, RenderError> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > 200
        || name.chars().any(|ch| {
            ch.is_control() || matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        || name == "."
        || name == ".."
    {
        return Err(RenderError::InvalidPayload(
            "文件名必须是安全的单个文件名".into(),
        ));
    }
    let file_name = match std::path::Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
    {
        Some(extension) if extension.eq_ignore_ascii_case(format.extension()) => Ok(name.into()),
        Some(_) => Err(RenderError::InvalidPayload(format!(
            "文件扩展名必须为 .{}",
            format.extension()
        ))),
        None => Ok(format!("{name}.{}", format.extension())),
    }?;
    if file_name.chars().count() > 200 {
        return Err(RenderError::InvalidPayload(
            "文件名不能超过200个字符（包含扩展名）".into(),
        ));
    }
    Ok(file_name)
}

pub fn render_markdown(file_name: &str, text: String) -> RenderedDeliverable {
    RenderedDeliverable {
        file_name: file_name.into(),
        mime_type: "text/markdown".into(),
        bytes: text.into_bytes(),
    }
}

pub fn render_json(
    file_name: &str,
    value: &serde_json::Value,
) -> Result<RenderedDeliverable, RenderError> {
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|error| RenderError::Encode(error.to_string()))?;
    Ok(RenderedDeliverable {
        file_name: file_name.into(),
        mime_type: "application/json".into(),
        bytes,
    })
}

pub fn render_document_markdown(
    file_name: &str,
    document: &DocumentContent,
) -> RenderedDeliverable {
    let mut text = format!("# {}\n", document.title);
    for section in &document.sections {
        text.push_str(&format!("\n## {}\n", section.heading));
        for paragraph in &section.paragraphs {
            text.push_str(&format!("\n{paragraph}\n"));
        }
        if let Some(rows) = &section.table {
            if let Some(header) = rows.first() {
                text.push_str(&format!("\n| {} |\n", header.join(" | ")));
                text.push_str(&format!(
                    "|{}|\n",
                    header.iter().map(|_| " --- ").collect::<Vec<_>>().join("|")
                ));
                for row in rows.iter().skip(1) {
                    text.push_str(&format!("| {} |\n", row.join(" | ")));
                }
            }
        }
    }
    if !document.sources.is_empty() {
        text.push_str("\n## 来源\n");
        for source in &document.sources {
            text.push_str(&format!("\n- {source}"));
        }
        text.push('\n');
    }
    render_markdown(file_name, text)
}

pub fn render_docx(
    file_name: &str,
    document: &DocumentContent,
) -> Result<RenderedDeliverable, RenderError> {
    let mut body = String::new();
    body.push_str(&paragraph_xml(&document.title, Some("Title")));
    for section in &document.sections {
        body.push_str(&paragraph_xml(&section.heading, Some("Heading1")));
        for paragraph in &section.paragraphs {
            body.push_str(&paragraph_xml(paragraph, None));
        }
        if let Some(rows) = &section.table {
            body.push_str("<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr>");
            for row in rows {
                body.push_str("<w:tr>");
                for cell in row {
                    body.push_str("<w:tc>");
                    body.push_str(&paragraph_xml(cell, None));
                    body.push_str("</w:tc>");
                }
                body.push_str("</w:tr>");
            }
            body.push_str("</w:tbl>");
        }
    }
    if !document.sources.is_empty() {
        body.push_str(&paragraph_xml("来源", Some("Heading1")));
        for source in &document.sources {
            body.push_str(&paragraph_xml(source, None));
        }
    }
    let document_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
    );
    let files = [
        (
            "[Content_Types].xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/></Types>".to_owned(),
        ),
        (
            "_rels/.rels",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>".to_owned(),
        ),
        (
            "word/_rels/document.xml.rels",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/></Relationships>".to_owned(),
        ),
        (
            "word/styles.xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style><w:style w:type=\"paragraph\" w:styleId=\"Title\"><w:name w:val=\"Title\"/><w:basedOn w:val=\"Normal\"/><w:uiPriority w:val=\"10\"/></w:style><w:style w:type=\"paragraph\" w:styleId=\"Heading1\"><w:name w:val=\"heading 1\"/><w:basedOn w:val=\"Normal\"/><w:uiPriority w:val=\"9\"/><w:qFormat/></w:style></w:styles>".to_owned(),
        ),
        ("word/document.xml", document_xml),
    ];
    Ok(RenderedDeliverable {
        file_name: file_name.into(),
        mime_type: DOCX_MIME.into(),
        bytes: zip_package(files.iter().map(|(name, value)| (*name, value.as_bytes())))?,
    })
}

pub fn render_xlsx(
    file_name: &str,
    sheet: &SheetContent,
) -> Result<RenderedDeliverable, RenderError> {
    validate_sheet(sheet)?;
    let mut sheet_xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>");
    sheet_xml.push_str("<row r=\"1\">");
    for (column, header) in sheet.headers.iter().enumerate() {
        sheet_xml.push_str(&inline_cell(&cell_ref(column, 1), header));
    }
    sheet_xml.push_str("</row>");
    for (row_index, row) in sheet.rows.iter().enumerate() {
        let excel_row = row_index + 2;
        sheet_xml.push_str(&format!("<row r=\"{excel_row}\">"));
        for (column, value) in row.iter().enumerate() {
            let reference = cell_ref(column, excel_row);
            match value {
                serde_json::Value::Null => {}
                serde_json::Value::Bool(boolean) => sheet_xml.push_str(&format!(
                    "<c r=\"{reference}\" t=\"b\"><v>{}</v></c>",
                    u8::from(*boolean)
                )),
                serde_json::Value::Number(number) => {
                    sheet_xml.push_str(&format!("<c r=\"{reference}\"><v>{}</v></c>", number))
                }
                serde_json::Value::String(text) => {
                    sheet_xml.push_str(&inline_cell(&reference, text))
                }
                serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                    let text = serde_json::to_string(value)
                        .map_err(|error| RenderError::Encode(error.to_string()))?;
                    sheet_xml.push_str(&inline_cell(&reference, &text));
                }
            }
        }
        sheet_xml.push_str("</row>");
    }
    sheet_xml.push_str("</sheetData></worksheet>");

    let workbook_xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"{}\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>",
        escape_xml(&sheet.name)
    );
    let files = [
        (
            "[Content_Types].xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>".to_owned(),
        ),
        (
            "_rels/.rels",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>".to_owned(),
        ),
        ("xl/workbook.xml", workbook_xml),
        (
            "xl/_rels/workbook.xml.rels",
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>".to_owned(),
        ),
        ("xl/worksheets/sheet1.xml", sheet_xml),
    ];
    Ok(RenderedDeliverable {
        file_name: file_name.into(),
        mime_type: XLSX_MIME.into(),
        bytes: zip_package(files.iter().map(|(name, value)| (*name, value.as_bytes())))?,
    })
}

pub fn render_png(
    file_name: &str,
    chart: &ChartContent,
) -> Result<RenderedDeliverable, RenderError> {
    if chart.categories.is_empty()
        || chart.categories.len() > 64
        || chart.categories.len() != chart.values.len()
        || chart.values.iter().any(|value| !value.is_finite())
    {
        return Err(RenderError::InvalidPayload(
            "PNG柱状图需要1到64个等长 categories 和有限数值 values".into(),
        ));
    }
    let mut canvas =
        image::RgbImage::from_pixel(PNG_WIDTH, PNG_HEIGHT, image::Rgb([255, 255, 255]));
    let left = 54i32;
    let right = PNG_WIDTH as i32 - 24;
    let top = 28i32;
    let bottom = PNG_HEIGHT as i32 - 38;
    let low = chart.values.iter().copied().fold(0.0f64, f64::min);
    let high = chart.values.iter().copied().fold(0.0f64, f64::max);
    let span = (high - low).max(1.0);
    let y_for = |value: f64| -> i32 {
        top + (((high - value) / span) * f64::from(bottom - top)).round() as i32
    };
    let zero_y = y_for(0.0);
    draw_line(&mut canvas, left, top, left, bottom, [90, 102, 116]);
    draw_line(&mut canvas, left, zero_y, right, zero_y, [90, 102, 116]);
    let slot = (right - left) as usize / chart.values.len();
    let bar_width = slot.saturating_mul(3) / 5;
    for (index, value) in chart.values.iter().copied().enumerate() {
        let center = left + (slot * index + slot / 2) as i32;
        let value_y = y_for(value);
        let x0 = center - (bar_width as i32 / 2);
        let x1 = center + (bar_width as i32 / 2).max(1);
        let y0 = value_y.min(zero_y);
        let y1 = value_y.max(zero_y);
        fill_rect(&mut canvas, x0, y0, x1, y1, [36, 133, 151]);
    }
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(canvas)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|error| RenderError::Encode(error.to_string()))?;
    Ok(RenderedDeliverable {
        file_name: file_name.into(),
        mime_type: "image/png".into(),
        bytes: bytes.into_inner(),
    })
}

pub fn render_pdf(
    _file_name: &str,
    _document: &DocumentContent,
) -> Result<RenderedDeliverable, RenderError> {
    let configured = std::env::var_os("YILIAN_PDF_RENDERER_PATH").is_some();
    let message = if configured {
        "YILIAN_PDF_RENDERER_PATH is configured, but artifact generation has no approved SecurityExecutionGateway hook; no process was started and no PDF was written".to_owned()
    } else {
        "PDF generation is unavailable: set YILIAN_PDF_RENDERER_PATH to a local renderer and provide an approved SecurityExecutionGateway hook; no PDF was written".to_owned()
    };
    Err(RenderError::PdfRendererIntegrationUnavailable(message))
}

fn paragraph_xml(text: &str, style: Option<&str>) -> String {
    let style = style
        .map(|value| format!("<w:pPr><w:pStyle w:val=\"{value}\"/></w:pPr>"))
        .unwrap_or_default();
    format!(
        "<w:p>{style}<w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        escape_xml(text)
    )
}

fn inline_cell(reference: &str, text: &str) -> String {
    format!(
        "<c r=\"{reference}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
        escape_xml(text)
    )
}

fn validate_sheet(sheet: &SheetContent) -> Result<(), RenderError> {
    if sheet.name.trim().is_empty()
        || sheet.name.chars().count() > 31
        || sheet.name.starts_with('\'')
        || sheet.name.ends_with('\'')
        || sheet
            .name
            .chars()
            .any(|ch| matches!(ch, '[' | ']' | ':' | '*' | '?' | '/' | '\\'))
    {
        return Err(RenderError::InvalidPayload(
            "工作表名称必须符合Excel限制（1到31个字符，且不得包含 []:*?/\\ 或首尾单引号）".into(),
        ));
    }
    if sheet.headers.is_empty() || sheet.headers.len() > 16_384 {
        return Err(RenderError::InvalidPayload(
            "工作表必须有1到16384列表头".into(),
        ));
    }
    if sheet.rows.len() > 1_048_575 || sheet.rows.iter().any(|row| row.len() > sheet.headers.len())
    {
        return Err(RenderError::InvalidPayload(
            "工作表行数或列数超出Excel范围".into(),
        ));
    }
    Ok(())
}

fn cell_ref(column_zero_based: usize, row: usize) -> String {
    let mut value = column_zero_based + 1;
    let mut letters = Vec::new();
    while value > 0 {
        let remainder = (value - 1) % 26;
        letters.push((b'A' + remainder as u8) as char);
        value = (value - 1) / 26;
    }
    letters.reverse();
    format!("{}{row}", letters.into_iter().collect::<String>())
}

fn draw_line(canvas: &mut image::RgbImage, x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 3]) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut error = dx + dy;
    let (mut x, mut y) = (x0, y0);
    loop {
        if x >= 0 && y >= 0 && x < canvas.width() as i32 && y < canvas.height() as i32 {
            canvas.put_pixel(x as u32, y as u32, image::Rgb(color));
        }
        if x == x1 && y == y1 {
            break;
        }
        let doubled = 2 * error;
        if doubled >= dy {
            error += dy;
            x += sx;
        }
        if doubled <= dx {
            error += dx;
            y += sy;
        }
    }
}

fn fill_rect(canvas: &mut image::RgbImage, x0: i32, y0: i32, x1: i32, y1: i32, color: [u8; 3]) {
    for y in y0.clamp(0, canvas.height() as i32)..=y1.clamp(0, canvas.height() as i32 - 1) {
        for x in x0.clamp(0, canvas.width() as i32)..=x1.clamp(0, canvas.width() as i32 - 1) {
            canvas.put_pixel(x as u32, y as u32, image::Rgb(color));
        }
    }
}

fn zip_package<'a>(
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Result<Vec<u8>, RenderError> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in files {
        archive
            .start_file(name, zip::write::SimpleFileOptions::default())
            .map_err(|error| RenderError::Encode(error.to_string()))?;
        archive
            .write_all(bytes)
            .map_err(|error| RenderError::Encode(error.to_string()))?;
    }
    archive
        .finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|error| RenderError::Encode(error.to_string()))
}

fn escape_xml(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&apos;".to_owned(),
            _ => character.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests;
