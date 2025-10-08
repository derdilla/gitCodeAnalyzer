use gix::date::Time;
use gix::object::tree::diff::Action;
use gix::Repository;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

#[allow(dead_code)]

/// Analyzer that clones and analyzes git repositories
pub struct Analyzer {
    repo: Repository,
    temp_dir: Option<PathBuf>,
}

impl Analyzer {
    /// Clone a repository from a URL into a temporary directory
    pub fn clone_from_url(url: &str) -> Result<Self, Box<dyn Error>> {
        let temp_dir = std::env::temp_dir().join(format!("git-analyzer-{}", uuid()));
        fs::create_dir_all(&temp_dir)?;

        let repo = gix::prepare_clone(url, &temp_dir)?
            .fetch_then_checkout(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)?
            .0
            .main_worktree(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)?
            .0;

        Ok(Self {
            repo,
            temp_dir: Some(temp_dir),
        })
    }

    /// Open an existing repository from a local path
    pub fn open_local(path: impl AsRef<Path>) -> Result<Self, Box<dyn Error>> {
        let repo = gix::open(path.as_ref())?;
        Ok(Self {
            repo,
            temp_dir: None,
        })
    }

    /// Get an iterator over all commits in the repository (starting from HEAD)
    pub fn commits(&self) -> Result<CommitIterator<'_>, Box<dyn Error>> {
        let mut head = self.repo.head()?;
        let head_commit = head.peel_to_commit_in_place()?;
        let ancestors = head_commit.ancestors().all()?;

        Ok(CommitIterator {
            repo: &self.repo,
            ancestors,
        })
    }

    /// Get the total count of commits in the repository (starting from HEAD)
    pub fn commit_count(&self) -> Result<usize, Box<dyn Error>> {
        let mut head = self.repo.head()?;
        let head_commit = head.peel_to_commit_in_place()?;
        let count = head_commit.ancestors().all()?.count();
        Ok(count)
    }
}

impl Drop for Analyzer {
    fn drop(&mut self) {
        if let Some(temp_dir) = &self.temp_dir {
            let _ = fs::remove_dir_all(temp_dir);
        }
    }
}

/// Iterator over commits in a repository
pub struct CommitIterator<'a> {
    repo: &'a Repository,
    ancestors: gix::revision::Walk<'a>,
}

impl<'a> Iterator for CommitIterator<'a> {
    type Item = Result<CommitAnalyzer<'a>, Box<dyn Error>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.ancestors.next() {
            Some(Ok(info)) => {
                Some(CommitAnalyzer::new(self.repo, info.id))
            }
            Some(Err(e)) => Some(Err(e.into())),
            None => None,
        }
    }
}

/// Provides analysis capabilities for a specific commit
pub struct CommitAnalyzer<'a> {
    repo: &'a Repository,
    commit: gix::Commit<'a>,
}

impl<'a> CommitAnalyzer<'a> {
    fn new(repo: &'a Repository, commit_id: gix::ObjectId) -> Result<Self, Box<dyn Error>> {
        let commit = repo.find_commit(commit_id)?;
        Ok(Self { repo, commit })
    }

    /// Get the commit ID
    pub fn id(&self) -> gix::ObjectId {
        self.commit.id
    }

    /// Get the commit message
    pub fn message(&self) -> Result<String, Box<dyn Error>> {
        Ok(self.commit.message()?.title.to_string())
    }

    /// Get the commit author
    pub fn author(&'_ self) -> Result<gix::actor::SignatureRef<'_>, Box<dyn Error>> {
        Ok(self.commit.author()?)
    }

    /// Get the commit time
    pub fn commit_time(&self) -> Result<Time, Box<dyn Error>> {
        Ok(self.commit.time()?)
    }

    /// Get the tree for this commit
    pub fn tree(&self) -> Result<gix::Tree<'a>, Box<dyn Error>> {
        Ok(self.commit.tree()?)
    }

    /// Iterate over all files in the commit tree
    pub fn iter_files<F>(&self, mut callback: F) -> Result<(), Box<dyn Error>>
    where
        F: FnMut(&FileEntry) -> Result<(), Box<dyn Error>>,
    {
        let tree = self.tree()?;
        let mut recorder = gix::traverse::tree::Recorder::default();
        tree.traverse().breadthfirst(&mut recorder)?;

        for record in recorder.records {
            if record.mode.is_blob() {
                let entry = FileEntry {
                    path: record.filepath.to_string(),
                    oid: record.oid,
                    mode: record.mode,
                };
                callback(&entry)?;
            }
        }

        Ok(())
    }

    /// Read the contents of a file in this commit
    pub fn read_file(&self, oid: gix::ObjectId) -> Result<Vec<u8>, Box<dyn Error>> {
        let object = self.repo.find_object(oid)?;
        Ok(object.data.to_vec())
    }

    /// Compare this commit with its parent to get diff statistics
    pub fn diff_stats(&self) -> Result<Option<DiffStats>, Box<dyn Error>> {
        let parent_ids: Vec<_> = self.commit.parent_ids().collect();

        if parent_ids.is_empty() {
            return Ok(None);
        }

        let parent = self.repo.find_commit(parent_ids[0])?;
        let parent_tree = parent.tree()?;
        let current_tree = self.tree()?;

        let mut changes = current_tree.changes()?;
        let stats = changes.stats(&parent_tree)?;

        Ok(Some(DiffStats {
            lines_added: stats.lines_added,
            lines_removed: stats.lines_removed,
            files_changed: stats.files_changed as usize,
        }))
    }

    /// Get detailed file changes between this commit and its parent
    pub fn file_changes<F>(&self, mut callback: F) -> Result<(), Box<dyn Error>>
    where
        F: FnMut(FileChange) -> Result<(), Box<dyn Error>>,
    {
        let parent_ids: Vec<_> = self.commit.parent_ids().collect();
        let current_tree = self.tree()?;

        if parent_ids.is_empty() {
            // No parent: this is the initial commit, report all files as added
            self.iter_files(|entry| {
                callback(FileChange::Added {
                    path: entry.path.clone(),
                    oid: entry.oid,
                })?;
                Ok(())
            })?;
            return Ok(());
        }

        let parent = self.repo.find_commit(parent_ids[0])?;
        let parent_tree = parent.tree()?;

        current_tree
            .changes()?
            .for_each_to_obtain_tree(&parent_tree, |change| {
                let file_change = match change {
                    gix::object::tree::diff::Change::Addition { location, id, .. } => {
                        FileChange::Added {
                            path: location.to_string(),
                            oid: id.to_owned().into(),
                        }
                    }
                    gix::object::tree::diff::Change::Deletion { location, id, .. } => {
                        FileChange::Deleted {
                            path: location.to_string(),
                            oid: id.to_owned().into(),
                        }
                    }
                    gix::object::tree::diff::Change::Modification { location, previous_id, id, .. } => {
                        FileChange::Modified {
                            path: location.to_string(),
                            old_oid: previous_id.to_owned().into(),
                            new_oid: id.to_owned().into(),
                        }
                    }
                    gix::object::tree::diff::Change::Rewrite { source_location: _, location, id, .. } => {
                        // TODO: fix handling when moving code in and out of tests
                        FileChange::Modified {
                            path: location.to_string(),
                            old_oid: id.to_owned().into(),
                            new_oid: id.to_owned().into(),
                        }
                    }
                };

                callback(file_change).ok();
                Ok::<_, std::convert::Infallible>(Action::Continue)
            })?;

        Ok(())
    }
}

/// Information about a file in the repository
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: String,
    pub oid: gix::ObjectId,
    pub mode: gix::object::tree::EntryMode,
}

/// Statistics about changes in a commit
#[derive(Debug, Clone)]
pub struct DiffStats {
    pub lines_added: u64,
    pub lines_removed: u64,
    pub files_changed: usize,
}

/// Represents a change to a file
#[derive(Debug, Clone)]
pub enum FileChange {
    Added { path: String, oid: gix::ObjectId },
    Deleted { path: String, oid: gix::ObjectId },
    Modified { path: String, old_oid: gix::ObjectId, new_oid: gix::ObjectId },
}

// Simple UUID generator for temp directories
fn uuid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", nanos)
}

#[derive(Debug)]
pub struct ChangeAnalyzer {
    current_commit: CommitChangeAnalysis,
    analyzed_commits: Vec<CommitChangeAnalysis>,
}

impl ChangeAnalyzer {
    pub fn new() -> Self {
        Self {
            current_commit: CommitChangeAnalysis::new(Time::default()),
            analyzed_commits: Vec::new(),
        }
    }

    pub fn start_commit(&mut self, commit: &CommitAnalyzer) {
        self.analyzed_commits.push(self.current_commit.clone());
        self.current_commit.info = CommitInfo {
            time: commit.commit_time().unwrap(),
        };
    }

    pub fn handle_change(&mut self, change: FileChange, analyzer: &CommitAnalyzer) -> Result<(), Box<dyn Error>> {
        match change {
            FileChange::Added { path, oid } => { self.handle_added(&path, oid, analyzer) }
            FileChange::Deleted { path, oid } => { self.handle_deleted(&path, oid, analyzer) }
            FileChange::Modified { path, old_oid, new_oid } => self.handle_edited(&path, old_oid, new_oid, analyzer),
        }
    }

    fn handle_added(&mut self, path: &String, oid: gix::ObjectId, analyzer: &CommitAnalyzer) -> Result<(), Box<dyn Error>> {
        if let Some(language) = Language::from_path(path) {
            let content = analyzer.read_file(oid)?;
            let stats = language.analyze_content(&content, path);
            *self.current_commit.loc.entry(language).or_default() += stats;
        }
        Ok(())
    }

    fn handle_deleted(&mut self, path: &String, oid: gix::ObjectId, analyzer: &CommitAnalyzer) -> Result<(), Box<dyn Error>> {
        if let Some(language) = Language::from_path(path) {
            let content = analyzer.read_file(oid)?;
            let stats = language.analyze_content(&content, path);
            *self.current_commit.loc.entry(language).or_default() -= stats;
        }
        Ok(())
    }

    fn handle_edited(&mut self, path: &String, old_oid: gix::ObjectId, new_oid: gix::ObjectId, analyzer: &CommitAnalyzer) -> Result<(), Box<dyn Error>> {
        self.handle_deleted(path, old_oid, analyzer)?;
        self.handle_added(path, new_oid, analyzer)
    }

    /// Returns commit analysis including the current one, sorted by time.
    pub fn analysis(&self) -> Vec<CommitChangeAnalysis> {
        let mut commits = self.analyzed_commits.clone();
        commits.push(self.current_commit.clone());
        commits.sort_by(|a, b| a.info.time.cmp(&b.info.time));
        commits
    }
}

#[derive(Debug, Clone)]
pub struct CommitChangeAnalysis {
    pub loc: HashMap<Language, LanguageStats>,
    pub info: CommitInfo,
}

#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub time: Time,
}

impl CommitChangeAnalysis {
    fn new(time: Time) -> Self {
        Self {
            loc: HashMap::new(),
            info: CommitInfo { time },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Dart,
    Python,
    Java,
    C,
    Cpp,
    Other,
}

#[derive(Debug, Clone, Default)]
pub struct LanguageStats {
    pub total_lines: usize,
    pub test_lines: usize,
    pub comment_lines: usize,
}

impl std::ops::Add for LanguageStats {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        Self {
            total_lines: self.total_lines + other.total_lines,
            test_lines: self.test_lines + other.test_lines,
            comment_lines: self.comment_lines + other.comment_lines,
        }
    }
}

impl std::ops::AddAssign for LanguageStats {
    fn add_assign(&mut self, other: Self) {
        self.total_lines += other.total_lines;
        self.test_lines += other.test_lines;
        self.comment_lines += other.comment_lines;
    }
}

impl std::ops::Sub for LanguageStats {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        Self {
            total_lines: self.total_lines.saturating_sub(other.total_lines),
            test_lines: self.test_lines.saturating_sub(other.test_lines),
            comment_lines: self.comment_lines.saturating_sub(other.comment_lines),
        }
    }
}

impl std::ops::SubAssign for LanguageStats {
    fn sub_assign(&mut self, other: Self) {
        self.total_lines = self.total_lines.saturating_sub(other.total_lines);
        self.test_lines = self.test_lines.saturating_sub(other.test_lines);
        self.comment_lines = self.comment_lines.saturating_sub(other.comment_lines);
    }
}

impl Language {
    /// Detect language from file path
    fn from_path(path: &str) -> Option<Self> {
        if path.ends_with(".rs") {
            Some(Language::Rust)
        } else if path.ends_with(".py") {
            Some(Language::Python)
        } else if path.ends_with(".dart") {
            Some(Language::Dart)
        } else if path.ends_with(".java") {
            Some(Language::Java)
        } else if path.ends_with(".c") || path.ends_with(".h") {
            Some(Language::C)
        } else if path.ends_with(".cpp") || path.ends_with(".cc") || path.ends_with(".cxx")
                || path.ends_with(".hpp") || path.ends_with(".hxx") {
            Some(Language::Cpp)
        } else {
            Some(Language::Other)
        }
    }

    /// Check if path indicates a test file
    fn is_test_path(&self, path: &str) -> bool {
        match self {
            Language::Rust => {
                // Rust tests can be in tests/ directory or in test modules
                path.starts_with("tests/") || path.contains("/tests/")
            }
            Language::Python => {
                // Python tests usually in test_ files or tests/ directory
                path.contains("test_") || path.contains("/test/") || path.contains("/tests/")
                    || path.starts_with("test_") || path.starts_with("tests/")
            }
            Language::Dart => {
                // Dart tests in test/ directory or _test.dart files
                path.starts_with("test/") || path.contains("/test/") || path.ends_with("_test.dart")
            }
            Language::Java => {
                // Java tests usually in src/test/ or with Test suffix
                path.contains("/test/") || path.contains("Test.java") || path.ends_with("Tests.java")
            }
            Language::C | Language::Cpp => {
                // C/C++ tests usually in test/ directory or with test prefix/suffix
                path.contains("/test/") || path.contains("/tests/")
                    || path.starts_with("test_") || path.contains("_test.")
            }
            Language::Other => false,
        }
    }

    /// Analyze file content and return language stats
    fn analyze_content(&self, content: &[u8], path: &str) -> LanguageStats {
        let is_test_file = self.is_test_path(path);
        let content_str = String::from_utf8_lossy(content);
        let lines: Vec<&str> = content_str.lines().collect();

        let mut stats = LanguageStats {
            total_lines: lines.len(),
            test_lines: 0,
            comment_lines: 0,
        };

        match self {
            Language::Rust => {
                let mut in_test_module = false;
                let mut in_multiline_comment = false;
                let mut brace_depth = 0;
                let mut test_module_depth = 0;

                for line in &lines {
                    let trimmed = line.trim();

                    // Handle multi-line comments
                    if trimmed.starts_with("/*") {
                        in_multiline_comment = true;
                        stats.comment_lines += 1;
                        if trimmed.contains("*/") {
                            in_multiline_comment = false;
                        }
                        continue;
                    }

                    if in_multiline_comment {
                        stats.comment_lines += 1;
                        if trimmed.contains("*/") {
                            in_multiline_comment = false;
                        }
                        continue;
                    }

                    // Single-line comments
                    if trimmed.starts_with("//") {
                        stats.comment_lines += 1;
                        continue;
                    }

                    // Track test modules
                    if trimmed.contains("#[cfg(test)]") || trimmed == "#[test]" {
                        in_test_module = true;
                        test_module_depth = brace_depth;
                    }

                    // Track braces for module depth
                    brace_depth += trimmed.matches('{').count();

                    if in_test_module {
                        stats.test_lines += 1;
                    }

                    brace_depth = brace_depth.saturating_sub(trimmed.matches('}').count());

                    // Exit test module when closing brace at same depth
                    if in_test_module && brace_depth <= test_module_depth {
                        in_test_module = false;
                    }
                }

                // If entire file is a test file, count all non-comment lines as test lines
                if is_test_file && stats.test_lines == 0 {
                    stats.test_lines = stats.total_lines - stats.comment_lines;
                }
            }
            Language::Python => {
                let mut in_multiline_string = false;
                let mut multiline_delimiter = "";

                for line in &lines {
                    let trimmed = line.trim();

                    // Handle multi-line strings (docstrings)
                    if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
                        if !in_multiline_string {
                            in_multiline_string = true;
                            multiline_delimiter = if trimmed.starts_with("\"\"\"") { "\"\"\"" } else { "'''" };
                            stats.comment_lines += 1;
                            if trimmed.matches(multiline_delimiter).count() >= 2 {
                                in_multiline_string = false;
                            }
                            continue;
                        }
                    }

                    if in_multiline_string {
                        stats.comment_lines += 1;
                        if trimmed.contains(multiline_delimiter) {
                            in_multiline_string = false;
                        }
                        continue;
                    }

                    // Single-line comments
                    if trimmed.starts_with("#") {
                        stats.comment_lines += 1;
                        continue;
                    }

                    // Test detection
                    if is_test_file || trimmed.starts_with("def test_") || trimmed.contains("@pytest.") {
                        stats.test_lines += 1;
                    }
                }

                if is_test_file {
                    stats.test_lines = stats.total_lines - stats.comment_lines;
                }
            }
            Language::Dart | Language::Java | Language::C | Language::Cpp => {
                // C-style comments for Dart, Java, C, C++
                let mut in_multiline_comment = false;

                for line in &lines {
                    let trimmed = line.trim();

                    // Multi-line comments
                    if trimmed.starts_with("/*") {
                        in_multiline_comment = true;
                        stats.comment_lines += 1;
                        if trimmed.contains("*/") {
                            in_multiline_comment = false;
                        }
                        continue;
                    }

                    if in_multiline_comment {
                        stats.comment_lines += 1;
                        if trimmed.contains("*/") {
                            in_multiline_comment = false;
                        }
                        continue;
                    }

                    // Single-line comments
                    if trimmed.starts_with("//") {
                        stats.comment_lines += 1;
                        continue;
                    }

                    // Test detection
                    if is_test_file {
                        stats.test_lines += 1;
                    }
                }

                if is_test_file {
                    stats.test_lines = stats.total_lines - stats.comment_lines;
                }
            }
            Language::Other => {
                stats.total_lines += lines.len();
            }
        }

        stats
    }
}

impl Display for Language {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let txt = match self {
            Language::Rust => { "Rust" }
            Language::Dart => { "Dart" }
            Language::Python => { "Python" }
            Language::Java => { "Java" }
            Language::C => { "C" }
            Language::Cpp => { "Cpp" }
            Language::Other => { "Other" }
        };
        f.write_str(txt)?;
        Ok(())
    }
}
