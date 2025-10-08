use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use crate::analyzer::Language;

#[allow(dead_code)]

pub fn write_to_console(commits: Vec<crate::analyzer::CommitChangeAnalysis>) {
    println!("time, language, total, test, comment");
    for commit in commits {
        for (language, stats) in commit.loc {
            println!("{}, {}, {}, {}, {}", commit.info.time, language.to_string(), stats.total_lines, stats.test_lines, stats.comment_lines);
        }
    }
}

pub fn write_to_dir(commits: Vec<crate::analyzer::CommitChangeAnalysis>, dir: &PathBuf) {
    fs::create_dir_all(dir).unwrap();
    let mut writer_map = std::collections::HashMap::new();
    writer_map.insert(Language::Rust, File::create(dir.join(format!("{}.csv", Language::Rust))).unwrap());
    writer_map.insert(Language::Dart, File::create(dir.join(format!("{}.csv", Language::Dart))).unwrap());
    writer_map.insert(Language::Python, File::create(dir.join(format!("{}.csv", Language::Python))).unwrap());
    writer_map.insert(Language::Java, File::create(dir.join(format!("{}.csv", Language::Java))).unwrap());
    writer_map.insert(Language::C, File::create(dir.join(format!("{}.csv", Language::C))).unwrap());
    writer_map.insert(Language::Cpp, File::create(dir.join(format!("{}.csv", Language::Cpp))).unwrap());
    writer_map.insert(Language::Other, File::create(dir.join(format!("{}.csv", Language::Other))).unwrap());

    for writer in writer_map.values_mut() {
        writer.write("time, language, total, test, comment\n".as_bytes()).unwrap();
    }
    for commit in commits {
        for (language, stats) in commit.loc {
            writer_map.get_mut(&language).unwrap().write(
                format!("{}, {}, {}, {}, {}\n", commit.info.time, language.to_string(), stats.total_lines, stats.test_lines, stats.comment_lines).as_bytes()
            ).unwrap();
        }
    }
}