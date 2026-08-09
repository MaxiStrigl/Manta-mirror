fn main() {
    let src_dir = std::path::Path::new("vendor/tree-sitter-org/src");

    println!("cargo:rerun-if-changed={}", src_dir.display());

    cc::Build::new()
        .include(src_dir)
        .include(src_dir.join("tree_sitter"))
        .file(src_dir.join("parser.c"))
        .file(src_dir.join("scanner.c"))
        .compile("tree-sitter-org");
}
