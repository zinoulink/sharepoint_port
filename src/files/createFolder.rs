use async_trait::async_trait;
use regex::Regex;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FolderError {
    #[error("Invalid folder path: {0}")]
    InvalidPath(String),
    #[error("Folder already exists: {0}")]
    DuplicateFolder(String),
    #[error("SharePoint error: {0}")]
    SharePointError(String),
    #[error("Unknown error occurred")]
    Unknown,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FolderObject {
    fs_obj_type: i32,
    base_name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AddResult {
    passed: Vec<FolderObject>,
    failed: Vec<FailedOperation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FailedOperation {
    base_name: String,
    error_message: String,
}

#[async_trait]
pub trait SharePointAdd {
    async fn add(&self, items: Vec<FolderObject>) -> Result<AddResult, FolderError>;
}

pub struct FolderCreator<T: SharePointAdd> {
    add_client: T,
}

impl<T: SharePointAdd> FolderCreator<T> {
    pub fn new(add_client: T) -> Self {
        Self { add_client }
    }

    /// Creates a folder structure in SharePoint based on the provided path
    pub async fn create_folder(&self, folder_path: &str) -> Result<FolderObject, FolderError> {
        // Validate input
        if folder_path.trim().is_empty() {
            return Err(FolderError::InvalidPath(
                "Folder path cannot be empty".to_string(),
            ));
        }

        // Normalize path and create folder objects
        let normalized_path = self.normalize_path(folder_path)?;
        let folder_objects = self.generate_folder_objects(&normalized_path)?;

        // Attempt to create folders
        let result = self
            .add_client
            .add(folder_objects)
            .await
            .map_err(|e| FolderError::SharePointError(e.to_string()))?;

        // Handle the creation result
        self.handle_creation_result(result, &normalized_path)
    }

    /// Normalizes the folder path by removing invalid characters and formatting
    fn normalize_path(&self, path: &str) -> Result<String, FolderError> {
        let invalid_chars = Regex::new(r"[\*\?\|:\"'<>#{}%~&]").map_err(|e| {
            FolderError::InvalidPath(format!("Failed to compile regex pattern: {}", e))
        })?;
        let multiple_spaces = Regex::new(r" {2,}").unwrap();
        let multiple_dots = Regex::new(r"\.{2,}").unwrap();
        let leading_trailing_dots_spaces = Regex::new(r"^[\. ]+|[\. ]+$").unwrap();

        let normalized = invalid_chars
            .replace_all(path, "")
            .trim_matches('/')
            .to_string();
        let normalized = leading_trailing_dots_spaces.replace_all(&normalized, "");
        let normalized = multiple_spaces.replace_all(&normalized, " ");
        let normalized = multiple_dots.replace_all(&normalized, ".");

        Ok(normalized.to_string())
    }

    /// Generates folder objects for each level of the path
    fn generate_folder_objects(&self, path: &str) -> Result<Vec<FolderObject>, FolderError> {
        let segments: Vec<&str> = path.split('/').collect();
        let mut folder_objects = Vec::with_capacity(segments.len());

        let mut current_path = String::new();
        for (i, segment) in segments.iter().enumerate() {
            if i > 0 {
                current_path.push('/');
            }
            current_path.push_str(segment);

            folder_objects.push(FolderObject {
                fs_obj_type: 1,
                base_name: current_path.clone(),
            });
        }

        Ok(folder_objects)
    }

    /// Handles the result of the folder creation attempt
    fn handle_creation_result(
        &self,
        result: AddResult,
        target_path: &str,
    ) -> Result<FolderObject, FolderError> {
        // Check successful creations
        if let Some(success) = result
            .passed
            .into_iter()
            .find(|item| item.base_name == target_path)
        {
            return Ok(success);
        }

        // Check failed creations
        if let Some(failed) = result
            .failed
            .into_iter()
            .find(|item| item.base_name == target_path)
        {
            // Handle duplicate folder case
            if failed.error_message.contains("0x8107090d") {
                return Err(FolderError::DuplicateFolder(target_path.to_string()));
            }
            return Err(FolderError::SharePointError(failed.error_message));
        }

        Err(FolderError::Unknown)
    }
}

// Example implementation of SharePointAdd trait
#[cfg(test)]
mod tests {
    use super::*;

    struct MockAddClient;

    #[async_trait]
    impl SharePointAdd for MockAddClient {
        async fn add(&self, _items: Vec<FolderObject>) -> Result<AddResult, FolderError> {
            // Mock implementation for testing
            Ok(AddResult {
                passed: vec![FolderObject {
                    fs_obj_type: 1,
                    base_name: "test/folder".to_string(),
                }],
                failed: vec![],
            })
        }
    }

    #[tokio::test]
    async fn test_create_folder() {
        let creator = FolderCreator::new(MockAddClient);
        let result = creator.create_folder("test/folder").await;
        assert!(result.is_ok());
    }
}