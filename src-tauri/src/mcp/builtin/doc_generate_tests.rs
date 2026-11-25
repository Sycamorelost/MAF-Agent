//! `doc_generate` 的无网络测试:用 Mock Provider 验证两段子调用与降级。

use std::collections::VecDeque;
use std::io::Read;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use futures::stream;
use serde_json::json;

use crate::provider::{ChatRequest, ChatStream, Provider, ProviderError, StreamChunk, SubLlm};

use super::{
    doc_generate::{audit_document, clean_markdown, generate},
    execute_builtin,
};

#[derive(Default)]
struct MockState {
    replies: VecDeque<Result<String, String>>,
    prompts: Vec<String>,
}

#[derive(Clone, Default)]
struct MockProvider(Arc<Mutex<MockState>>);

impl MockProvider {
    fn with_replies(replies: Vec<Result<&str, &str>>) -> Self {
        let state = MockState {
            replies: replies
                .into_iter()
                .map(|r| r.map(str::to_string).map_err(str::to_string))
                .collect(),
            prompts: Vec::new(),
        };
        Self(Arc::new(Mutex::new(state)))
    }

    fn llm(&self) -> SubLlm {
        SubLlm {
            provider: Box::new(self.clone()),
            model: "mock-model".into(),
        }
    }

    fn prompts(&self) -> Vec<String> {
        self.0.lock().unwrap().prompts.clone()
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn chat_stream(&self, req: ChatRequest) -> Result<ChatStream, ProviderError> {
        let reply = {
            let mut state = self.0.lock().unwrap();
            state.prompts.push(
                req.messages
                    .iter()
                    .map(|m| format!("{}:{}", m.role, m.content))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
            state.replies.pop_front().expect("测试应预备足够响应")
        };
        let text = reply.map_err(ProviderError::Other)?;
        Ok(Box::pin(stream::iter(vec![
            StreamChunk::Delta { content: text },
            StreamChunk::Done {
                finish_reason: Some("stop".into()),
                prompt_tokens: None,
                completion_tokens: None,
                reasoning_tokens: None,
                cached_tokens: None,
            },
        ])))
    }
}

#[tokio::test]
async fn writer_then_reviewer_returns_revised_body() {
    let provider = MockProvider::with_replies(vec![
        Ok("## 初稿\n\n根据素材得到结论。"),
        Ok("## 修订稿\n\n根据素材得到结论,并去除重复。"),
    ]);
    let generated = generate(&provider.llm(), "标题", "写报告", "素材 A")
        .await
        .unwrap();
    assert!(generated.reviewed);
    assert_eq!(generated.body, "## 修订稿\n\n根据素材得到结论,并去除重复。");
    assert!(generated.warnings.is_empty());

    let prompts = provider.prompts();
    assert_eq!(prompts.len(), 2);
    assert!(prompts[0].starts_with("system:你是文档 Writer"));
    assert!(prompts[1].starts_with("system:你是文档 Reviewer"));
    assert!(prompts[1].contains("【初稿】\n## 初稿"));
    assert!(prompts.iter().all(|p| p.contains("user:【文档标题】")));
}

#[tokio::test]
async fn builtin_exports_revised_docx() {
    let provider = MockProvider::with_replies(vec![
        Ok("## 初稿\n\n不应落盘的初稿内容。"),
        Ok("## 结论\n\n审校后的唯一正文。"),
    ]);
    let llm = provider.llm();
    let dir = std::env::temp_dir().join(format!("maf-doc-generate-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("report.docx");
    let (content, ok) = execute_builtin(
        "doc_generate",
        &json!({
            "path": path.to_string_lossy(),
            "title": "测试报告",
            "brief": "依据素材生成简短报告",
            "sources": "结论只能来自素材。",
            "template": "summary"
        }),
        false,
        None,
        Some(&llm),
    )
    .await;
    assert!(ok, "全链路导出失败:{content}");
    let result: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert_eq!(result["generated_by"], "writer_reviewer");
    assert_eq!(result["reviewed"], true);
    assert_eq!(result["template"], "工作总结");
    assert!(result["bytes"].as_u64().unwrap() > 0);

    let bytes = std::fs::read(&path).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    zip.by_name("word/document.xml")
        .unwrap()
        .read_to_string(&mut xml)
        .unwrap();
    assert!(xml.contains("审校后的唯一正文"));
    assert!(!xml.contains("不应落盘的初稿内容"));
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn reviewer_failure_keeps_draft_with_explicit_warning() {
    let provider = MockProvider::with_replies(vec![Ok("## 初稿\n\n正文"), Err("模拟限流")]);
    let generated = generate(&provider.llm(), "标题", "写报告", "")
        .await
        .unwrap();
    assert!(!generated.reviewed);
    assert_eq!(generated.body, "## 初稿\n\n正文");
    assert_eq!(generated.warnings.len(), 1);
    assert!(generated.warnings[0].contains("Reviewer 未完成"));
    assert!(generated.warnings[0].contains("模拟限流"));
}

#[test]
fn markdown_fence_cleanup_does_not_eat_code_blocks() {
    assert_eq!(clean_markdown("```markdown\n## 正文\n```"), "## 正文");
    let code = "```rust\nfn main() {}\n```";
    assert_eq!(clean_markdown(code), code);
}

#[test]
fn audit_reports_exact_duplicates_and_unresolved_markers() {
    let paragraph = "这是一段用于验证重复审计的正文,长度超过二十四个字符,包含素材结论。";
    let body =
        format!("## 章节\n\n{paragraph}\n\n{paragraph}\n\n另一节仍需补充数据,标记为待补充。");
    let audit = audit_document(&body);
    assert_eq!(audit.headings, 1);
    assert_eq!(audit.duplicate_paragraphs.len(), 1);
    assert_eq!(audit.unresolved_markers, 1);
    assert_eq!(audit.characters, body.chars().count());
}
