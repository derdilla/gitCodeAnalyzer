use std::path::PathBuf;

pub mod analyzer;
mod output_writer;

// TODO: Fix analysis
// - recheck code
// - Why are there still some 0 outliers?

fn main() {
    println!("Checking out repository...");
    //let repo = analyzer::Analyzer::open_local(PathBuf::from("./testrepo"));
    let repo = analyzer::Analyzer::clone_from_url("https://github.com/derdilla/blood-pressure-monitor-fl.git");
    //let repo = analyzer::Analyzer::clone_from_url("https://github.com/derdilla/aosp-analyzer.git");
    let repo = repo.unwrap();

    let mut analyzer= analyzer::ChangeAnalyzer::new();
    let total_commit_count = repo.commit_count().unwrap();
    let mut commit_idx = 0;
    for commit in repo.commits().unwrap() {
        commit_idx += 1;
        let commit = commit.unwrap();
        println!("Processing {}/{}: {}", commit_idx, total_commit_count, commit.message().unwrap());
        analyzer.start_commit(&commit);
        commit.file_changes(|c| analyzer
            .handle_change(c, &commit)).unwrap();
    }

    println!("Generating report...");
    output_writer::write_to_dir(analyzer.analysis(), &PathBuf::from("./testoutput"));
}
