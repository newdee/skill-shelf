use skill_shelf_core::{FileInput, Shelf, DEFAULT_BRANCH};

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut p = std::env::temp_dir();
    p.push(format!("skillshelf-{tag}-{}-{}", std::process::id(), nanos));
    p
}

fn file(path: &str, content: &[u8]) -> FileInput {
    FileInput {
        path: path.to_string(),
        content: content.to_vec(),
    }
}

#[test]
fn create_commit_history_diff_loop() {
    let dir = temp_dir("loop");
    let shelf = Shelf::open(&dir).unwrap();

    // create
    let skill = shelf
        .create_skill("pdf-parse", "Parse PDF files into text")
        .unwrap();
    assert_eq!(shelf.list_skills().unwrap().len(), 1);
    assert_eq!(shelf.get_skill(&skill.id).unwrap().name, "pdf-parse");

    // two commits, changing only run.py the second time
    let v1 = [
        file("SKILL.md", b"---\nname: pdf-parse\n---\n# v1"),
        file("scripts/run.py", b"print('v1')\n"),
    ];
    let c1 = shelf.commit(&skill.id, DEFAULT_BRANCH, &v1, "di@x.ai", "init").unwrap();

    let v2 = [
        file("SKILL.md", b"---\nname: pdf-parse\n---\n# v1"),
        file("scripts/run.py", b"print('v2')\n"),
    ];
    let c2 = shelf.commit(&skill.id, DEFAULT_BRANCH, &v2, "di@x.ai", "bump").unwrap();

    // history walks the parent chain, newest first
    let hist = shelf.list_commits(&skill.id, DEFAULT_BRANCH).unwrap();
    assert_eq!(hist.len(), 2);
    assert_eq!(hist[0].id, c2.id);
    assert_eq!(hist[0].parent.as_deref(), Some(c1.id.as_str()));
    assert_eq!(hist[1].id, c1.id);
    assert!(hist[1].parent.is_none());

    // content is retrievable at each version
    assert_eq!(shelf.read_file(&c1.id, "scripts/run.py").unwrap(), b"print('v1')\n");
    assert_eq!(shelf.read_file(&c2.id, "scripts/run.py").unwrap(), b"print('v2')\n");
    assert_eq!(shelf.read_tree(&c2.id).unwrap().len(), 2);

    // diff: run.py modified, SKILL.md unchanged
    let d = shelf.diff(&c1.id, &c2.id).unwrap();
    let by_path = |p: &str| d.iter().find(|f| f.path == p).unwrap();
    assert_eq!(by_path("scripts/run.py").status, "modified");
    assert_eq!(by_path("SKILL.md").status, "unchanged");
    assert!(by_path("SKILL.md").diff.is_none());
    assert!(by_path("scripts/run.py")
        .diff
        .as_ref()
        .unwrap()
        .contains("+print('v2')"));

    // rollback to c1 records a new head with c1's tree
    let c3 = shelf.rollback(&skill.id, DEFAULT_BRANCH, &c1.id, "di@x.ai").unwrap();
    assert_eq!(shelf.read_file(&c3.id, "scripts/run.py").unwrap(), b"print('v1')\n");
    let hist2 = shelf.list_commits(&skill.id, DEFAULT_BRANCH).unwrap();
    assert_eq!(hist2.len(), 3);
    assert_eq!(hist2[0].id, c3.id);

    // branch off main, inheriting its head
    let br = shelf.create_branch(&skill.id, "v2", Some(DEFAULT_BRANCH)).unwrap();
    assert_eq!(br.head.as_deref(), Some(c3.id.as_str()));
    assert_eq!(shelf.list_branches(&skill.id).unwrap().len(), 2);

    // delete removes it
    shelf.delete_skill(&skill.id).unwrap();
    assert!(shelf.get_skill(&skill.id).is_err());

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn add_and_remove_files_are_detected() {
    let dir = temp_dir("addrm");
    let shelf = Shelf::open(&dir).unwrap();
    let s = shelf.create_skill("x", "y").unwrap();

    let c1 = shelf
        .commit(&s.id, DEFAULT_BRANCH, &[file("a.txt", b"a")], "u", "1")
        .unwrap();
    let c2 = shelf
        .commit(&s.id, DEFAULT_BRANCH, &[file("b.txt", b"b")], "u", "2")
        .unwrap();

    let d = shelf.diff(&c1.id, &c2.id).unwrap();
    let by_path = |p: &str| d.iter().find(|f| f.path == p).unwrap();
    assert_eq!(by_path("a.txt").status, "removed");
    assert_eq!(by_path("b.txt").status, "added");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn route_ranks_relevant_skill_first() {
    let dir = temp_dir("route");
    let shelf = Shelf::open(&dir).unwrap();
    shelf
        .create_skill("pdf-parse", "Parse PDF files and extract text and tables")
        .unwrap();
    shelf
        .create_skill("image-resize", "Resize and crop images, convert formats")
        .unwrap();
    let target = shelf
        .create_skill("csv-clean", "Clean and normalize messy CSV spreadsheet data")
        .unwrap();

    // natural-language need → best match first
    let hits = shelf.route("help me tidy up a csv spreadsheet", 3).unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0].skill_id, target.id);

    // top_k = 1 → single skill
    let one = shelf.route("extract text from a pdf", 1).unwrap();
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].name, "pdf-parse");

    // no lexical overlap → no matches (safe on arbitrary input)
    assert!(shelf.route("!!! @@@ ???", 5).unwrap().is_empty());

    // deletion drops it from the index
    shelf.delete_skill(&target.id).unwrap();
    let after = shelf.route("csv spreadsheet", 5).unwrap();
    assert!(after.iter().all(|r| r.skill_id != target.id));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn feedback_and_refine_merge() {
    let dir = temp_dir("feedback");
    let shelf = Shelf::open(&dir).unwrap();
    let s = shelf.create_skill("pdf-parse", "parse pdfs").unwrap();

    // main v1
    let c1 = shelf
        .commit(&s.id, DEFAULT_BRANCH, &[file("SKILL.md", b"v1")], "u", "init")
        .unwrap();

    // feedback accumulates
    shelf
        .add_feedback(&s.id, Some(&c1.id), "agent", -1, "missed tables", Some("extract tables"))
        .unwrap();
    shelf.add_feedback(&s.id, None, "human", 1, "works great", None).unwrap();
    assert_eq!(shelf.list_feedback(&s.id).unwrap().len(), 2);
    assert_eq!(shelf.open_feedback(&s.id).unwrap().len(), 2);

    // refine: base a `refine` branch on main head, draft a new version there
    shelf.point_branch(&s.id, "refine", Some(&c1.id)).unwrap();
    shelf
        .commit(&s.id, "refine", &[file("SKILL.md", b"v2 with tables")], "ai", "refine draft")
        .unwrap();
    // main is untouched
    assert_eq!(shelf.read_file(&c1.id, "SKILL.md").unwrap(), b"v1");
    assert_eq!(shelf.list_commits(&s.id, DEFAULT_BRANCH).unwrap().len(), 1);

    // merge refine → main, then mark feedback applied
    let merged = shelf.merge_branch(&s.id, "refine", DEFAULT_BRANCH, "ai").unwrap();
    assert_eq!(shelf.read_file(&merged.id, "SKILL.md").unwrap(), b"v2 with tables");
    assert_eq!(shelf.list_commits(&s.id, DEFAULT_BRANCH).unwrap().len(), 2);
    shelf.mark_feedback_applied(&s.id).unwrap();
    assert_eq!(shelf.open_feedback(&s.id).unwrap().len(), 0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn by_name_and_search() {
    let dir = temp_dir("search");
    let shelf = Shelf::open(&dir).unwrap();
    shelf.create_skill("pdf-parse", "Parse PDF files").unwrap();
    shelf.create_skill("csv-clean", "Clean CSV spreadsheets").unwrap();
    shelf
        .create_skill_with_kind("summarize", "Summarize text", "prompt")
        .unwrap();

    // exact by name
    assert_eq!(shelf.get_skill_by_name("csv-clean").unwrap().name, "csv-clean");
    assert!(shelf.get_skill_by_name("nope").is_err());

    // substring search across name+description
    let hits = shelf.search_skills(Some("csv"), None, 50, 0).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name, "csv-clean");

    // kind filter
    let prompts = shelf.search_skills(None, Some("prompt"), 50, 0).unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].name, "summarize");

    // pagination
    assert_eq!(shelf.search_skills(None, None, 2, 0).unwrap().len(), 2);
    assert_eq!(shelf.search_skills(None, None, 2, 2).unwrap().len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_skill_name_is_rejected() {
    let dir = temp_dir("empty");
    let shelf = Shelf::open(&dir).unwrap();
    assert!(shelf.create_skill("  ", "d").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
