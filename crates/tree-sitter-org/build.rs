fn main() {
    let src_dir = std::path::Path::new("vendor/tree-sitter-org/src");

    println!("cargo:rerun-if-changed={}", src_dir.display());

    cc::Build::new()
        .include(src_dir)
        .include(src_dir.join("tree_sitter"))
        .file(src_dir.join("parser.c"))
        .file(src_dir.join("scanner.c"))
        .warnings(false)
        .compile("tree-sitter-org");

    treesitter_types::codegen::emit_to_out_dir(src_dir.join("node-types.json")).unwrap();
}
