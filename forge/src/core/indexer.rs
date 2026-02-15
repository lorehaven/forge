use anyhow::Result;
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, INDEXED, STORED, STRING, Schema, TEXT, Value};
use tantivy::{Index, IndexWriter, ReloadPolicy, TantivyDocument, Term};
use walkdir::WalkDir;

#[derive(Debug)]
pub struct Indexer {
    index: Index,
    path_field: Field,
    content_field: Field,
    modified_field: Field,
}

impl Indexer {
    pub fn new(index_path: &Path) -> Result<Self> {
        let mut schema_builder = Schema::builder();
        let path_field = schema_builder.add_text_field("path", STRING | STORED);
        let content_field = schema_builder.add_text_field("content", TEXT | STORED);
        let modified_field = schema_builder.add_i64_field("modified", STORED | INDEXED);
        let schema = schema_builder.build();

        if !index_path.exists() {
            std::fs::create_dir_all(index_path)?;
        }

        let index =
            Index::open_or_create(tantivy::directory::MmapDirectory::open(index_path)?, schema)?;

        Ok(Self {
            index,
            path_field,
            content_field,
            modified_field,
        })
    }

    pub fn index_project(&self, project_root: &Path) -> Result<()> {
        let mut index_writer: IndexWriter = self.index.writer(50_000_000)?; // 50MB heap

        for entry in WalkDir::new(project_root)
            .into_iter()
            .filter_entry(|e| !is_ignored(e))
            .filter_map(std::result::Result::ok)
        {
            if entry.file_type().is_file() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    let ext = ext.to_string_lossy().to_lowercase();
                    // Add more file extensions to the list
                    if matches!(
                        ext.as_str(),
                        "rs" | "toml" | "md" | "js" | "ts" | "py" | "c" | "cpp" | "h" | "hpp" |
                        "java" | "go" | "swift" | "php" | "rb" | "sh" | "pl" | "r" | "dart" | "scala"
                    ) {
                        let _ = self.index_file(&index_writer, project_root, path);
                    }
                }
            }
        }

        index_writer.commit()?;
        Ok(())
    }

    fn index_file(&self, writer: &IndexWriter, root: &Path, path: &Path) -> Result<()> {
        let relative_path = path.strip_prefix(root)?.to_string_lossy().to_string();
        let content = std::fs::read_to_string(path)?;
        let modified = std::fs::metadata(path)?
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs()
            .cast_signed();

        let term = Term::from_field_text(self.path_field, &relative_path);
        writer.delete_term(term);

        let mut doc = TantivyDocument::default();
        doc.add_text(self.path_field, relative_path);
        doc.add_text(self.content_field, content);
        doc.add_i64(self.modified_field, modified);

        writer.add_document(doc)?;
        Ok(())
    }

    pub fn search(&self, query_str: &str, limit: usize) -> Result<Vec<(String, String)>> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        let searcher = reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.content_field]);
        let query = query_parser.parse_query(query_str)?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;

        let mut results = Vec::new();
        for (_score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;
            let path = retrieved_doc
                .get_first(self.path_field)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let content = retrieved_doc
                .get_first(self.content_field)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            results.push((path, content));
        }

        Ok(results)
    }
}

fn is_ignored(entry: &walkdir::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .is_some_and(|s| s == "target" || s == ".git" || s == "node_modules" || s == ".ferrous")
}
