use super::protocol;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub(crate) const PROMPT: &str = r#"
当用户希望查看图表、结构化表格、可填写表单、任务状态或可审查的页面修改建议，可在普通 Markdown 中插入单独一段 ```yilian-rich 围栏（结束行为 ```）。围栏中只写 schema_version=1 的 JSON 对象，blocks 是下述对象数组。每个对象具有唯一 id，不需要填充任意状态、URL、脚本或工具调用。
- 文本 {"type":"text","id":"intro","text":"Markdown"}
- 表格 {"type":"table","id":"table","title":"结果","columns":["名称","数值"],"rows":[["A","3"],["B","5"]]}
- 图表 {"type":"chart","id":"chart","title":"结果","kind":"bar","labels":["A","B"],"values":[3,5]}，kind 可以为 bar、line 或 scatter。
- 表单 {"type":"form","id":"survey","title":"反馈","fields":[{"name":"note","label":"备注","required":true}]}，选填 choices 可提供字符串选项。
- 卡片 {"type":"interactive_card","id":"card","title":"确认事项","text":"描述","action_label":"标为已查看"}，仅记录用户已阅。
- 图片或文件 {"type":"image","id":"image","resource_id":"已登记的本对话资源 ID"}，文件使用 file。只能引用本对话可用的真实资源。
- 任务 {"type":"task_status","id":"task","task_id":"当前工作空间的真实任务 ID"}，状态由系统读取。
- 页面编辑提案 {"type":"page_proposal","id":"edit","page_id":"用户指定页面 ID","revision":1,"changes":[{"block_id":"被替换的 Markdown 块 ID","markdown":"修改后内容"},{"reference_page_id":"用户提供的同工作空间页面 ID"}]}，省略 block_id 可追加 Markdown。只在用户提供页面及其版本时生成提案，最终应用由用户审查并确认。
- 新建页面提案 {"type":"page_create_proposal","id":"new-page","title":"页面标题","markdown":"页面正文","parent_page_id":"用户提供的父页面 ID，可省略"}，由用户审查后创建。
"#;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ChartKind {
    Bar,
    Line,
    Scatter,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FormField {
    pub(super) name: String,
    pub(super) label: String,
    #[serde(default)]
    pub(super) required: bool,
    #[serde(default)]
    pub(super) choices: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged, deny_unknown_fields)]
pub(super) enum PageChange {
    Markdown {
        block_id: Option<String>,
        markdown: String,
    },
    Reference {
        reference_page_id: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Block {
    Text {
        id: String,
        text: String,
    },
    Code {
        id: String,
        language: String,
        code: String,
    },
    Math {
        id: String,
        expression: String,
    },
    Image {
        id: String,
        resource_id: String,
    },
    File {
        id: String,
        resource_id: String,
    },
    Artifact {
        id: String,
        resource_id: String,
    },
    Table {
        id: String,
        title: String,
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Chart {
        id: String,
        title: String,
        kind: ChartKind,
        labels: Vec<String>,
        values: Vec<f64>,
    },
    Form {
        id: String,
        title: String,
        fields: Vec<FormField>,
    },
    InteractiveCard {
        id: String,
        title: String,
        text: String,
        action_label: String,
    },
    TaskStatus {
        id: String,
        task_id: String,
    },
    PageProposal {
        id: String,
        page_id: String,
        revision: i64,
        changes: Vec<PageChange>,
    },
    PageCreateProposal {
        id: String,
        title: String,
        markdown: String,
        parent_page_id: Option<String>,
    },
}

impl Block {
    pub(super) fn id(&self) -> &str {
        match self {
            Self::Text { id, .. }
            | Self::Code { id, .. }
            | Self::Math { id, .. }
            | Self::Image { id, .. }
            | Self::File { id, .. }
            | Self::Artifact { id, .. }
            | Self::Table { id, .. }
            | Self::Chart { id, .. }
            | Self::Form { id, .. }
            | Self::InteractiveCard { id, .. }
            | Self::TaskStatus { id, .. }
            | Self::PageProposal { id, .. }
            | Self::PageCreateProposal { id, .. } => id,
        }
    }

    pub(super) fn action(&self) -> Option<&'static str> {
        match self {
            Self::Table { .. } => Some("sort"),
            Self::Chart { .. } => Some("select"),
            Self::Form { .. } => Some("submit"),
            Self::InteractiveCard { .. } => Some("acknowledge"),
            Self::PageProposal { .. } | Self::PageCreateProposal { .. } => {
                Some("apply_page_proposal")
            }
            _ => None,
        }
    }

    fn valid(&self) -> bool {
        valid_id(self.id())
            && match self {
                Self::Table { columns, rows, .. } => {
                    !columns.is_empty()
                        && columns.len() <= 30
                        && rows.len() <= 500
                        && rows.iter().all(|row| row.len() == columns.len())
                }
                Self::Chart { labels, values, .. } => {
                    !labels.is_empty()
                        && labels.len() == values.len()
                        && values.len() <= 500
                        && values
                            .iter()
                            .all(|value| value.is_finite() && value.abs() <= 1e100)
                }
                Self::Form { fields, .. } => {
                    let mut unique = HashSet::new();
                    !fields.is_empty()
                        && fields.len() <= 40
                        && fields.iter().all(|field| {
                            valid_id(&field.name)
                                && unique.insert(&field.name)
                                && !field.label.trim().is_empty()
                                && field.choices.len() <= 50
                                && field.choices.iter().all(|choice| !choice.is_empty())
                        })
                }
                Self::PageProposal {
                    page_id,
                    revision,
                    changes,
                    ..
                } => {
                    valid_id(page_id)
                        && *revision > 0
                        && !changes.is_empty()
                        && changes.len() <= 30
                        && changes.iter().all(|change| match change {
                            PageChange::Markdown { block_id, .. } => {
                                block_id.as_deref().is_none_or(valid_id)
                            }
                            PageChange::Reference { reference_page_id } => {
                                valid_id(reference_page_id)
                            }
                        })
                }
                Self::PageCreateProposal {
                    title,
                    parent_page_id,
                    ..
                } => !title.trim().is_empty() && parent_page_id.as_deref().is_none_or(valid_id),
                Self::Image { resource_id, .. }
                | Self::File { resource_id, .. }
                | Self::Artifact { resource_id, .. } => valid_id(resource_id),
                Self::TaskStatus { task_id, .. } => valid_id(task_id),
                _ => true,
            }
    }
}

pub(super) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Envelope {
    pub(super) schema_version: u32,
    pub(super) blocks: Vec<Block>,
}

#[derive(Clone, Copy)]
enum Format {
    Internal,
    AgUi,
    A2Ui,
}

impl Format {
    fn from_label(label: &str) -> Option<Self> {
        match label {
            "yilian-rich" => Some(Self::Internal),
            "ag-ui" => Some(Self::AgUi),
            "a2ui" => Some(Self::A2Ui),
            _ => None,
        }
    }

    fn decode(self, raw: &str) -> Option<Envelope> {
        if raw.len() > 256_000 {
            return None;
        }
        match self {
            Self::Internal => serde_json::from_str(raw).ok(),
            Self::AgUi => protocol::ag_ui(raw),
            Self::A2Ui => protocol::a2ui(raw),
        }
    }
}

#[derive(Debug)]
pub(super) enum Part {
    Text(String),
    Rich(Vec<Block>),
}

pub(super) fn parse_message(source: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let mut pending = String::new();
    let mut fence: Option<(u8, usize, Option<Format>)> = None;
    let mut ids = HashSet::new();
    let mut offset = 0;
    let mut json_start = 0;
    let mut fence_start = 0;
    for line in source.split_inclusive('\n') {
        let content = line.trim_end_matches(['\r', '\n']);
        let trimmed = content.trim_start_matches(' ');
        let indent = content.len() - trimmed.len();
        let marker = trimmed.as_bytes().first().copied().unwrap_or_default();
        let count = if matches!(marker, b'`' | b'~') {
            trimmed.bytes().take_while(|b| *b == marker).count()
        } else {
            0
        };
        let info = trimmed[count..].trim();
        let marker_line = indent <= 3 && count >= 3 && matches!(marker, b'`' | b'~');
        let position = offset;
        offset += line.len();
        if let Some((open_marker, open_count, format)) = fence {
            if marker_line && marker == open_marker && count >= open_count && info.is_empty() {
                fence = None;
                if let Some(format) = format {
                    let raw = &source[json_start..position];
                    let decoded = format.decode(raw);
                    let mut new_ids = HashSet::new();
                    if let Some(envelope) = decoded.filter(|value| {
                        value.schema_version == 1
                            && (1..=80).contains(&value.blocks.len())
                            && value.blocks.iter().all(|block| {
                                block.valid()
                                    && !ids.contains(block.id())
                                    && new_ids.insert(block.id().to_owned())
                            })
                    }) {
                        ids.extend(new_ids);
                        if !pending.trim().is_empty() {
                            parts.push(Part::Text(std::mem::take(&mut pending)));
                        }
                        parts.push(Part::Rich(envelope.blocks));
                    } else {
                        pending.push_str(&source[fence_start..offset]);
                    }
                    continue;
                }
            }
            if format.is_none() {
                pending.push_str(line);
            }
        } else if marker_line {
            let format = Format::from_label(info);
            fence = Some((marker, count, format));
            if format.is_some() {
                fence_start = position;
                json_start = offset;
            } else {
                pending.push_str(line);
            }
        } else {
            pending.push_str(line);
        }
    }
    if fence.is_some_and(|(_, _, format)| format.is_some()) {
        pending.push_str(&source[fence_start..]);
    }
    if !pending.trim().is_empty() {
        parts.push(Part::Text(pending));
    }
    parts
}
