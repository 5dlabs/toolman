use anyhow::Result;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskAnalysis {
    pub task_id: String,
    pub suggested_tools: Vec<ToolSuggestion>,
    pub existing_tags: Vec<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSuggestion {
    pub server_name: String,
    pub tool_name: String,
    pub reason: String,
    pub confidence: f32,
}

pub struct ToolSuggester {
    // Keyword mappings for different tools
    tool_keywords: HashMap<(String, String), Vec<String>>,
    // Pattern matchers
    patterns: Vec<(Regex, Vec<(String, String)>)>,
}

impl ToolSuggester {
    pub fn new() -> Self {
        let mut suggester = Self {
            tool_keywords: HashMap::new(),
            patterns: Vec::new(),
        };

        // Initialize keyword mappings
        suggester.init_keyword_mappings();
        suggester.init_pattern_matchers();

        suggester
    }

    fn init_keyword_mappings(&mut self) {
        // Filesystem tools
        self.tool_keywords.insert(
            ("filesystem".to_string(), "read_file".to_string()),
            vec![
                "read file",
                "load file",
                "open file",
                "file contents",
                "read from",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );

        self.tool_keywords.insert(
            ("filesystem".to_string(), "write_file".to_string()),
            vec![
                "write file",
                "save file",
                "create file",
                "write to",
                "save to",
                "generate file",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );

        self.tool_keywords.insert(
            ("filesystem".to_string(), "list_directory".to_string()),
            vec![
                "list files",
                "directory contents",
                "folder structure",
                "ls",
                "dir",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );

        // Git tools
        self.tool_keywords.insert(
            ("git".to_string(), "git_status".to_string()),
            vec![
                "git status",
                "check changes",
                "uncommitted",
                "modified files",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );

        self.tool_keywords.insert(
            ("git".to_string(), "git_commit".to_string()),
            vec!["commit", "git commit", "save changes", "checkpoint"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );

        // Memory tools
        self.tool_keywords.insert(
            ("memory".to_string(), "create_entities".to_string()),
            vec![
                "remember",
                "store information",
                "save to memory",
                "create entity",
                "knowledge graph",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );

        self.tool_keywords.insert(
            ("memory".to_string(), "read_graph".to_string()),
            vec![
                "recall",
                "retrieve memory",
                "what do you know",
                "read memory",
                "get information",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );

        // GitHub tools
        self.tool_keywords.insert(
            ("github".to_string(), "create_issue".to_string()),
            vec![
                "create issue",
                "github issue",
                "bug report",
                "feature request",
                "track issue",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );

        self.tool_keywords.insert(
            ("github".to_string(), "create_pull_request".to_string()),
            vec!["pull request", "PR", "merge request", "code review"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );

        // TaskMaster tools
        self.tool_keywords.insert(
            ("task-master-ai".to_string(), "get_tasks".to_string()),
            vec!["list tasks", "show tasks", "task status", "project status"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );

        self.tool_keywords.insert(
            ("task-master-ai".to_string(), "add_task".to_string()),
            vec!["add task", "create task", "new task", "task for"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );

        // Docker tools
        self.tool_keywords.insert(
            ("docker".to_string(), "list_containers".to_string()),
            vec![
                "docker ps",
                "list containers",
                "running containers",
                "docker status",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );

        self.tool_keywords.insert(
            ("docker".to_string(), "build_image".to_string()),
            vec!["docker build", "build image", "create image", "dockerfile"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );

        // Browser/Puppeteer tools
        self.tool_keywords.insert(
            ("puppeteer".to_string(), "screenshot".to_string()),
            vec![
                "screenshot",
                "capture page",
                "web screenshot",
                "browser capture",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );

        self.tool_keywords.insert(
            ("puppeteer".to_string(), "navigate".to_string()),
            vec![
                "navigate to",
                "open url",
                "browse to",
                "visit page",
                "web scraping",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        );
    }

    fn init_pattern_matchers(&mut self) {
        // Pattern for API/HTTP operations
        self.patterns.push((
            Regex::new(r"(?i)(api|http|rest|endpoint|webhook|request|response)").unwrap(),
            vec![("fetch".to_string(), "fetch".to_string())],
        ));

        // Pattern for database operations
        self.patterns.push((
            Regex::new(r"(?i)(database|postgres|mysql|sql|query|table|schema)").unwrap(),
            vec![("postgres".to_string(), "query".to_string())],
        ));

        // Pattern for Redis operations
        self.patterns.push((
            Regex::new(r"(?i)(redis|cache|key-value|session)").unwrap(),
            vec![
                ("redis".to_string(), "get".to_string()),
                ("redis".to_string(), "set".to_string()),
            ],
        ));

        // Pattern for testing
        self.patterns.push((
            Regex::new(r"(?i)(test|testing|unit test|integration test|e2e)").unwrap(),
            vec![("task-master-ai".to_string(), "add_task".to_string())],
        ));
    }

    pub fn analyze_task(&self, task_description: &str, task_details: Option<&str>) -> TaskAnalysis {
        let mut suggested_tools = Vec::new();
        let mut seen_tools = HashSet::new();

        // Combine description and details for analysis
        let full_text =
            format!("{} {}", task_description, task_details.unwrap_or("")).to_lowercase();

        // Extract existing tool tags
        let tag_regex = Regex::new(r"#tool:(\w+)").unwrap();
        let existing_tags: Vec<String> = tag_regex
            .captures_iter(&full_text)
            .map(|cap| cap[1].to_string())
            .collect();

        // Check keyword mappings
        for ((server, tool), keywords) in &self.tool_keywords {
            let tool_key = format!("{}_{}", server, tool);
            if seen_tools.contains(&tool_key) {
                continue;
            }

            let mut match_count = 0;
            let mut matched_keywords = Vec::new();

            for keyword in keywords {
                if full_text.contains(keyword) {
                    match_count += 1;
                    matched_keywords.push(keyword.clone());
                }
            }

            if match_count > 0 {
                let confidence = (match_count as f32 / keywords.len() as f32).min(1.0);
                suggested_tools.push(ToolSuggestion {
                    server_name: server.clone(),
                    tool_name: tool.clone(),
                    reason: format!("Matched keywords: {}", matched_keywords.join(", ")),
                    confidence,
                });
                seen_tools.insert(tool_key);
            }
        }

        // Check pattern matchers
        for (pattern, tools) in &self.patterns {
            if pattern.is_match(&full_text) {
                for (server, tool) in tools {
                    let tool_key = format!("{}_{}", server, tool);
                    if !seen_tools.contains(&tool_key) {
                        suggested_tools.push(ToolSuggestion {
                            server_name: server.clone(),
                            tool_name: tool.clone(),
                            reason: format!("Matched pattern: {}", pattern.as_str()),
                            confidence: 0.7,
                        });
                        seen_tools.insert(tool_key);
                    }
                }
            }
        }

        // Sort by confidence
        suggested_tools.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        // Calculate overall confidence
        let overall_confidence = if suggested_tools.is_empty() {
            0.0
        } else {
            suggested_tools.iter().map(|s| s.confidence).sum::<f32>() / suggested_tools.len() as f32
        };

        TaskAnalysis {
            task_id: String::new(), // Will be set by caller
            suggested_tools,
            existing_tags,
            confidence: overall_confidence,
        }
    }

    pub fn analyze_tasks(&self, tasks: &serde_json::Value) -> Result<Vec<TaskAnalysis>> {
        let mut analyses = Vec::new();

        if let Some(tasks_array) = tasks.as_array() {
            for task in tasks_array {
                if let (Some(id), Some(title)) = (
                    task.get("id").and_then(|v| v.as_str()),
                    task.get("title").and_then(|v| v.as_str()),
                ) {
                    let description = task.get("description").and_then(|v| v.as_str());
                    let details = task.get("details").and_then(|v| v.as_str());

                    let full_description =
                        format!("{} {}", description.unwrap_or(""), details.unwrap_or(""));

                    let mut analysis = self.analyze_task(title, Some(&full_description));
                    analysis.task_id = id.to_string();

                    // Also analyze subtasks if present
                    if let Some(subtasks) = task.get("subtasks").and_then(|v| v.as_array()) {
                        for subtask in subtasks {
                            if let (Some(sub_title), Some(sub_desc)) = (
                                subtask.get("title").and_then(|v| v.as_str()),
                                subtask.get("description").and_then(|v| v.as_str()),
                            ) {
                                let sub_analysis = self.analyze_task(sub_title, Some(sub_desc));
                                // Merge subtask suggestions into parent task
                                for suggestion in sub_analysis.suggested_tools {
                                    if !analysis.suggested_tools.iter().any(|s| {
                                        s.server_name == suggestion.server_name
                                            && s.tool_name == suggestion.tool_name
                                    }) {
                                        analysis.suggested_tools.push(suggestion);
                                    }
                                }
                            }
                        }
                    }

                    if !analysis.suggested_tools.is_empty() {
                        analyses.push(analysis);
                    }
                }
            }
        }

        Ok(analyses)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filesystem_suggestions() {
        let suggester = ToolSuggester::new();

        let analysis = suggester.analyze_task(
            "Read file config.json",
            Some("Need to read file and parse its contents"),
        );

        assert!(!analysis.suggested_tools.is_empty());
        assert!(analysis
            .suggested_tools
            .iter()
            .any(|s| s.server_name == "filesystem" && s.tool_name == "read_file"));
    }

    #[test]
    fn test_git_suggestions() {
        let suggester = ToolSuggester::new();

        let analysis = suggester.analyze_task(
            "Commit the changes",
            Some("Save all modified files to git with a descriptive message"),
        );

        assert!(analysis
            .suggested_tools
            .iter()
            .any(|s| s.server_name == "git" && s.tool_name == "git_commit"));
    }

    #[test]
    fn test_multiple_suggestions() {
        let suggester = ToolSuggester::new();

        let analysis = suggester.analyze_task(
            "Create API endpoint documentation",
            Some("Write documentation for the REST API endpoints and save to docs/api.md"),
        );

        // Should suggest both filesystem (write) and API-related tools
        assert!(analysis.suggested_tools.len() >= 2);
    }
}
