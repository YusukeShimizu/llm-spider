use std::time::Duration;

use clap::{Args, Parser, Subcommand};

use crate::openai::ReasoningEffort;

#[derive(Debug, Parser)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Spider(SpiderArgs),
}

#[derive(Debug, Args)]
pub struct SpiderArgs {
    #[arg(long)]
    pub query: String,

    #[arg(
        long,
        value_enum,
        help = "Reasoning effort (default: medium; env: LLM_SPIDER_OPENAI_REASONING_EFFORT)"
    )]
    pub reasoning_effort: Option<ReasoningEffort>,

    #[arg(long, default_value_t = 4000)]
    pub max_chars: usize,

    #[arg(long, default_value_t = 3)]
    pub min_sources: usize,

    #[arg(long, default_value_t = 10)]
    pub search_limit: usize,

    #[arg(long, default_value_t = 20)]
    pub max_pages: usize,

    #[arg(long, default_value_t = 1)]
    pub max_depth: usize,

    #[arg(
        long,
        value_parser = humantime::parse_duration,
        default_value = "30s"
    )]
    pub max_elapsed: Duration,

    #[arg(long, default_value_t = 20)]
    pub max_child_candidates: usize,

    #[arg(long, default_value_t = 3)]
    pub max_children_per_page: usize,

    #[arg(long, default_value_t = false)]
    pub allow_local: bool,
}
