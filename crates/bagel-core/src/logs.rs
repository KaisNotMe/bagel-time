//! Turning the game's console output into log lines.
//!
//! Modern versions (with Mojang's log config) print log4j XML events; old ones
//! print plain lines like `[12:00:00] [Client thread/INFO]: text`.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    /// "INFO", "WARN", "ERROR", "DEBUG", ...
    pub level: String,
    pub thread: Option<String>,
    /// Unix milliseconds, when the game reported it.
    pub timestamp: Option<u64>,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct LogParser {
    event: Option<String>,
}

impl LogParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one line of output. Returns a log line once one is complete.
    pub fn feed(&mut self, line: &str) -> Option<LogLine> {
        if let Some(buf) = &mut self.event {
            buf.push_str(line);
            buf.push('\n');
            if line.contains("</log4j:Event>") {
                let xml = self.event.take().unwrap_or_default();
                return Some(parse_event(&xml));
            }
            return None;
        }
        if line.trim_start().starts_with("<log4j:Event") {
            if line.contains("</log4j:Event>") {
                return Some(parse_event(line));
            }
            self.event = Some(format!("{line}\n"));
            return None;
        }
        if line.trim().is_empty() {
            return None;
        }
        Some(plain_line(line))
    }
}

fn plain_line(line: &str) -> LogLine {
    let level = ["FATAL", "ERROR", "WARN", "DEBUG"]
        .into_iter()
        .find(|l| line.contains(&format!("/{l}]")))
        .unwrap_or("INFO");
    LogLine {
        level: level.to_string(),
        thread: None,
        timestamp: None,
        message: line.to_string(),
    }
}

fn parse_event(xml: &str) -> LogLine {
    let mut message = cdata_in(xml, "log4j:Message").unwrap_or_default();
    if let Some(throwable) = cdata_in(xml, "log4j:Throwable") {
        if !message.is_empty() {
            message.push('\n');
        }
        message.push_str(throwable.trim_end());
    }
    LogLine {
        level: attr(xml, "level").unwrap_or_else(|| "INFO".to_string()),
        thread: attr(xml, "thread"),
        timestamp: attr(xml, "timestamp").and_then(|t| t.parse().ok()),
        message,
    }
}

fn attr(xml: &str, name: &str) -> Option<String> {
    let needle = format!(" {name}=\"");
    let start = xml.find(&needle)? + needle.len();
    let end = xml[start..].find('"')? + start;
    Some(unescape(&xml[start..end]))
}

fn cdata_in(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}><![CDATA[");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find("]]>")? + start;
    Some(xml[start..end].to_string())
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multi_line_xml_event() {
        let mut p = LogParser::new();
        let lines = [
            r#"<log4j:Event logger="ezc" timestamp="1791436665262" level="INFO" thread="Render thread">"#,
            r#"  <log4j:Message><![CDATA[Setting user: Steve]]></log4j:Message>"#,
            r#"</log4j:Event>"#,
        ];
        assert_eq!(p.feed(lines[0]), None);
        assert_eq!(p.feed(lines[1]), None);
        let line = p.feed(lines[2]).unwrap();
        assert_eq!(line.level, "INFO");
        assert_eq!(line.thread.as_deref(), Some("Render thread"));
        assert_eq!(line.timestamp, Some(1791436665262));
        assert_eq!(line.message, "Setting user: Steve");
    }

    #[test]
    fn event_with_throwable() {
        let mut p = LogParser::new();
        let xml = "<log4j:Event logger=\"x\" timestamp=\"1\" level=\"ERROR\" thread=\"main\">\n\
            <log4j:Message><![CDATA[Oops]]></log4j:Message>\n\
            <log4j:Throwable><![CDATA[java.lang.RuntimeException\n\tat a.b]]></log4j:Throwable>\n\
            </log4j:Event>";
        let mut out = None;
        for l in xml.lines() {
            out = p.feed(l).or(out);
        }
        let line = out.unwrap();
        assert_eq!(line.level, "ERROR");
        assert_eq!(line.message, "Oops\njava.lang.RuntimeException\n\tat a.b");
    }

    #[test]
    fn plain_lines() {
        let mut p = LogParser::new();
        let line = p.feed("[12:00:00] [Client thread/WARN]: Something odd").unwrap();
        assert_eq!(line.level, "WARN");
        assert_eq!(p.feed("Exception in thread main").unwrap().level, "INFO");
        assert_eq!(p.feed("   "), None);
    }
}
