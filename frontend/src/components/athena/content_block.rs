use super::ask_user_block::AskUserBlockView;
use super::eval_block::EvaluationBlockView;
use super::plan_block::PlanBlockView;
use crate::stores::athena::ContentBlock;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ContentBlockRendererProps {
    pub block: ContentBlock,
}

#[component]
pub fn ContentBlockRenderer(props: ContentBlockRendererProps) -> Element {
    match props.block {
        ContentBlock::Plan(plan) => rsx! { PlanBlockView { plan } },
        ContentBlock::AskUser(ask) => rsx! { AskUserBlockView { ask } },
        ContentBlock::Evaluation(eval) => rsx! { EvaluationBlockView { eval } },
    }
}
