use std::process::ExitCode;

use anyhow::Context as _;
use clap::Parser as _;

fn main() -> ExitCode {
    if let Err(err) = try_main() {
        eprintln!("{err:#}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

fn try_main() -> anyhow::Result<()> {
    llm_spider::logging::init().context("init logging")?;

    let cli = llm_spider::cli::Cli::parse();
    tracing::debug!(?cli, "parsed cli");

    match cli.command {
        llm_spider::cli::Command::Spider(args) => {
            let request = llm_spider::spider::UserRequest {
                query: args.query,
                max_chars: args.max_chars,
                min_sources: args.min_sources,
                search_limit: args.search_limit,
                max_pages: args.max_pages,
                max_depth: args.max_depth,
                max_elapsed: args.max_elapsed,
                max_child_candidates: args.max_child_candidates,
                max_children_per_page: args.max_children_per_page,
                allow_local: args.allow_local,
            };

            tracing::info!(
                query = %request.query,
                max_pages = request.max_pages,
                max_depth = request.max_depth,
                max_elapsed = ?request.max_elapsed,
                "spider start"
            );

            let openai = llm_spider::openai::OpenAiClient::from_env().context("init openai")?;
            let openai = match args.reasoning_effort {
                Some(effort) => openai.with_reasoning_effort(effort),
                None => openai,
            };
            let result = llm_spider::spider::crawl(&request, &openai).context("crawl")?;
            let markdown = llm_spider::spider::compose_markdown(&request, &result);
            print!("{markdown}");
        }
    }

    Ok(())
}
