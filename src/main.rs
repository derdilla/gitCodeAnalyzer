use std::error::Error;
use std::fs;
use gix::object::tree::diff::Stats;

fn main() {
    let loc = loc("https://github.com/derdilla/personal-website.git".to_string()).unwrap();
    println!("{}", loc);
}

fn loc(git_url: String) -> Result<String, Box<dyn Error>> {
    let git_root = std::env::temp_dir().join("derdilla.bawb-gwd");
    _ = fs::create_dir_all(&git_root);
    let repo = gix::prepare_clone(git_url, &git_root)
        .map(|mut repo| repo
            .fetch_then_checkout(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)
            .unwrap()
            .0
            .main_worktree(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)
            .unwrap()
            .0
        )
        .unwrap_or_else(|_| gix::open(&git_root).unwrap());

    let mut head = repo.head().unwrap();
    let head_commit = head.peel_to_commit_in_place().unwrap();
    let head_tree = head_commit.tree().unwrap();

    let mut deltas = vec![];

    // Iterate over ancestors (parents) of the HEAD commit, in reverse order
    let mut last = head_tree;
    for commit in head_commit.ancestors().all()? {
        let mut commit = commit.unwrap();
        //println!("Commit: {}", commit.id);
        //dbg!(commit.commit_time());
        if let Ok(treeCommit) = repo.find_commit(commit.id()) {
            if let Ok(tree) = treeCommit.tree() {
                let mut changes = tree.changes().unwrap();
                let stats = changes.stats(&last).unwrap();
                let x = stats.lines_added.abs_diff(stats.lines_removed);
                let sign = if stats.lines_added >= stats.lines_removed { "+" } else { "-" };
                deltas.push(format!("{sign}{x}"));
                last = tree;
                //dbg!(stats);
            } else { println!("no changes") }
        } else { println!("No tree to commit") }

    }

    Ok(deltas.join("\n"))
}
