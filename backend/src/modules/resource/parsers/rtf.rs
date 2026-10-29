//! RTF paragraphs and direct character formatting. Document layout is not reproduced.
use crate::modules::resource::model::{Presentation, PreviewBlock, PreviewRun};

#[derive(Clone)]
struct State {
    skip_group: bool,
    bold: bool,
    italic: bool,
    underline: bool,
    codepage: u16,
    unicode_skip: usize,
}
impl Default for State {
    fn default() -> Self {
        Self {
            skip_group: false,
            bold: false,
            italic: false,
            underline: false,
            codepage: 1252,
            unicode_skip: 1,
        }
    }
}
#[derive(Default)]
struct Output {
    blocks: Vec<PreviewBlock>,
    runs: Vec<PreviewRun>,
    bytes: Vec<u8>,
    surrogate: Option<u16>,
}
impl Output {
    fn text(&mut self, text: &str, state: &State) {
        if state.skip_group || text.is_empty() {
            return;
        }
        if let Some(last) = self.runs.last_mut() {
            if last.bold == state.bold
                && last.italic == state.italic
                && last.underline == state.underline
            {
                last.text.push_str(text);
                return;
            }
        }
        self.runs.push(PreviewRun {
            text: text.into(),
            bold: state.bold,
            italic: state.italic,
            underline: state.underline,
        });
    }
    fn flush(&mut self, state: &State) -> Result<(), String> {
        if self.bytes.is_empty() {
            return Ok(());
        }
        let encoding =
            codepage::to_encoding_no_replacement(state.codepage).ok_or("不支持的 RTF 代码页")?;
        let (text, invalid) = encoding.decode_without_bom_handling(&self.bytes);
        if invalid {
            return Err("RTF 字符编码无效".into());
        }
        let text = text.into_owned();
        self.bytes.clear();
        self.text(&text, state);
        Ok(())
    }
    fn unicode(&mut self, value: u16, state: &State) -> Result<(), String> {
        if (0xd800..=0xdbff).contains(&value) {
            self.surrogate = Some(value);
            return Ok(());
        }
        let value = if (0xdc00..=0xdfff).contains(&value) {
            let high = self.surrogate.take().ok_or("RTF Unicode 代理项无效")?;
            char::from_u32(0x10000 + (((high as u32) - 0xd800) << 10) + ((value as u32) - 0xdc00))
                .ok_or("RTF Unicode 无效")?
        } else {
            if self.surrogate.take().is_some() {
                return Err("RTF Unicode 代理项无效".into());
            }
            char::from_u32(value as u32).ok_or("RTF Unicode 无效")?
        };
        self.text(&value.to_string(), state);
        Ok(())
    }
    fn paragraph(&mut self) {
        let runs = std::mem::take(&mut self.runs);
        let text = runs.iter().map(|r| r.text.as_str()).collect::<String>();
        self.blocks.push(PreviewBlock {
            kind: "paragraph".into(),
            text: Some(text),
            runs: Some(runs),
            ..Default::default()
        });
    }
}

pub(crate) fn parse(input: &[u8]) -> Result<Presentation, String> {
    if !input.starts_with(b"{\\rtf") {
        return Err("RTF 文件头无效".into());
    }
    let mut state = State::default();
    let mut stack = Vec::new();
    let mut output = Output::default();
    let mut index = 0;
    let mut fallback = 0usize;
    while index < input.len() {
        let c = input[index];
        index += 1;
        match c {
            b'{' => {
                output.flush(&state)?;
                stack.push(state.clone());
            }
            b'}' => {
                output.flush(&state)?;
                state = stack.pop().ok_or("RTF 组层级无效")?;
            }
            b'\\' => {
                if index >= input.len() {
                    return Err("RTF 控制字无效".into());
                }
                let next = input[index];
                index += 1;
                if next == b'\'' {
                    if index + 2 > input.len() {
                        return Err("RTF 十六进制字符无效".into());
                    }
                    let hex = std::str::from_utf8(&input[index..index + 2])
                        .map_err(|_| "RTF 十六进制字符无效")?;
                    let byte = u8::from_str_radix(hex, 16).map_err(|_| "RTF 十六进制字符无效")?;
                    index += 2;
                    if fallback > 0 {
                        fallback -= 1;
                    } else if !state.skip_group {
                        output.bytes.push(byte);
                    }
                } else if !next.is_ascii_alphabetic() {
                    if fallback > 0 && matches!(next, b'\\' | b'{' | b'}' | b'~' | b'_' | b'-') {
                        fallback -= 1;
                        continue;
                    }
                    match next {
                        b'\\' | b'{' | b'}' => {
                            if !state.skip_group {
                                output.bytes.push(next);
                            }
                        }
                        b'*' => {
                            output.flush(&state)?;
                            state.skip_group = true;
                        }
                        b'~' | b'_' | b'-' => {
                            output.flush(&state)?;
                            output.text(
                                match next {
                                    b'~' => "\u{a0}",
                                    b'_' => "\u{2011}",
                                    _ => "\u{ad}",
                                },
                                &state,
                            );
                        }
                        _ => {}
                    }
                } else {
                    output.flush(&state)?;
                    let start = index - 1;
                    while index < input.len() && input[index].is_ascii_alphabetic() {
                        index += 1;
                    }
                    let word =
                        std::str::from_utf8(&input[start..index]).map_err(|_| "RTF 控制字无效")?;
                    let num_start = index;
                    if index < input.len() && input[index] == b'-' {
                        index += 1;
                    }
                    while index < input.len() && input[index].is_ascii_digit() {
                        index += 1;
                    }
                    let number = if index > num_start {
                        Some(
                            std::str::from_utf8(&input[num_start..index])
                                .map_err(|_| "RTF 数值无效")?
                                .parse::<i32>()
                                .map_err(|_| "RTF 数值无效")?,
                        )
                    } else {
                        None
                    };
                    if index < input.len() && input[index] == b' ' {
                        index += 1;
                    }
                    if matches!(
                        word,
                        "fonttbl"
                            | "colortbl"
                            | "stylesheet"
                            | "info"
                            | "pict"
                            | "object"
                            | "header"
                            | "footer"
                            | "headerl"
                            | "headerr"
                            | "footerl"
                            | "footerr"
                            | "filetbl"
                            | "listtable"
                            | "listoverridetable"
                            | "generator"
                            | "xmlnstbl"
                            | "datastore"
                            | "themedata"
                            | "nonshppict"
                            | "shppict"
                    ) {
                        state.skip_group = true;
                    }
                    if word == "bin" {
                        let n = number.ok_or("RTF 二进制长度无效")?;
                        if n < 0 || index + n as usize > input.len() {
                            return Err("RTF 二进制长度无效".into());
                        }
                        index += n as usize;
                    }
                    if state.skip_group {
                        continue;
                    }
                    match word {
                        "ansicpg" => {
                            state.codepage = u16::try_from(number.ok_or("RTF 代码页无效")?)
                                .map_err(|_| "RTF 代码页无效")?
                        }
                        "uc" => {
                            state.unicode_skip =
                                usize::try_from(number.ok_or("RTF Unicode 跳过数无效")?)
                                    .map_err(|_| "RTF Unicode 跳过数无效")?
                        }
                        "u" => {
                            let number = number.ok_or("RTF Unicode 无效")?;
                            if !(-32768..=65535).contains(&number) {
                                return Err("RTF Unicode 无效".into());
                            }
                            output.unicode(number as u16, &state)?;
                            fallback = state.unicode_skip;
                        }
                        "b" => state.bold = number != Some(0),
                        "i" => state.italic = number != Some(0),
                        "ul" => state.underline = number != Some(0),
                        "ulnone" => state.underline = false,
                        "plain" => {
                            state.bold = false;
                            state.italic = false;
                            state.underline = false;
                        }
                        "par" => output.paragraph(),
                        "line" => output.text("\n", &state),
                        "tab" => output.text("\t", &state),
                        "emdash" => output.text("—", &state),
                        "endash" => output.text("–", &state),
                        "bullet" => output.text("•", &state),
                        _ => {}
                    }
                }
            }
            b'\r' | b'\n' => {}
            _ => {
                if fallback > 0 {
                    fallback -= 1;
                } else if !state.skip_group {
                    output.bytes.push(c);
                }
            }
        }
    }
    if !stack.is_empty() {
        return Err("RTF 组未闭合".into());
    }
    output.flush(&state)?;
    if !output.runs.is_empty() {
        output.paragraph();
    }
    if output.surrogate.is_some() {
        return Err("RTF Unicode 代理项无效".into());
    }
    Ok(Presentation {
        blocks: output.blocks,
        ..Default::default()
    })
}
