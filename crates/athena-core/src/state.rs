use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    Claude,
    Codex,
    Gemini,
    Shell,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum GridTemplate {
    #[serde(rename = "1x1")]
    OneByOne,
    #[serde(rename = "1x2")]
    OneByTwo,
    #[serde(rename = "2x2")]
    TwoByTwo,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub grid: GridTemplate,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct WorkspaceState {
    pub spaces: Vec<Space>,
    pub active_space_id: Option<String>,
}

pub fn grid_for_pane_count(count: usize) -> GridTemplate {
    match count {
        0..=1 => GridTemplate::OneByOne,
        2 => GridTemplate::OneByTwo,
        _ => GridTemplate::TwoByTwo,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_grid_template_serialization() {
        let grid = GridTemplate::TwoByTwo;
        let serialized = serde_json::to_string(&grid).unwrap();
        assert_eq!(serialized, "\"2x2\"");
    }
}
