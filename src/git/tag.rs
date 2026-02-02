//! Tag info structure and operations

use anyhow::Result;
use git2::{Oid, Repository};

/// Git tag information
#[derive(Debug, Clone)]
pub struct TagInfo {
    /// Tag name (without refs/tags/ prefix)
    pub name: String,
    /// The OID this tag points to (commit or tag object)
    pub target_oid: Oid,
    /// Whether this is a lightweight tag (vs annotated)
    pub is_lightweight: bool,
    /// For annotated tags, the tagger name
    pub tagger_name: Option<String>,
    /// For annotated tags, the tag message
    pub message: Option<String>,
}

impl TagInfo {
    /// List all tags in the repository
    pub fn list_all(repo: &Repository) -> Result<Vec<Self>> {
        let mut tags = Vec::new();
        
        repo.tag_foreach(|oid, name_bytes| {
            // Parse tag name (strip refs/tags/ prefix)
            if let Ok(name_str) = std::str::from_utf8(name_bytes) {
                let name = name_str
                    .strip_prefix("refs/tags/")
                    .unwrap_or(name_str)
                    .to_string();
                
                // Try to peel to a commit to get the actual target
                if let Ok(reference) = repo.find_reference(name_str) {
                    if let Ok(commit) = reference.peel_to_commit() {
                        let target_oid = commit.id();
                        
                        // Check if this is an annotated tag
                        let (is_lightweight, tagger_name, message) = 
                            if let Ok(tag_obj) = repo.find_tag(oid) {
                                (
                                    false,
                                    tag_obj.tagger().and_then(|t| t.name().map(|s| s.to_string())),
                                    tag_obj.message().map(|s| s.to_string()),
                                )
                            } else {
                                (true, None, None)
                            };
                        
                        tags.push(TagInfo {
                            name,
                            target_oid,
                            is_lightweight,
                            tagger_name,
                            message,
                        });
                    }
                }
            }
            true // Continue iteration
        })?;
        
        // Sort by name for consistent ordering
        tags.sort_by(|a, b| a.name.cmp(&b.name));
        
        Ok(tags)
    }
    
    /// Build a map from commit OID to list of tag names
    pub fn build_commit_tag_map(tags: &[TagInfo]) -> std::collections::HashMap<Oid, Vec<String>> {
        let mut map = std::collections::HashMap::new();
        for tag in tags {
            map.entry(tag.target_oid)
                .or_insert_with(Vec::new)
                .push(tag.name.clone());
        }
        map
    }
}
