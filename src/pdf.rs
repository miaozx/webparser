//! PDF content extraction.
//!
//! Extracts text from PDF files with page limit support using `pdf_oxide`.

use crate::error::{Error, Result};
use pdf_oxide::PdfDocument;

/// Maximum number of pages to extract.
const MAX_PDF_PAGES: usize = 5;

/// Check if bytes look like a PDF (starts with `%PDF`).
pub fn is_pdf(data: &[u8]) -> bool {
    data.starts_with(b"%PDF")
}

/// Extract text from PDF bytes, limited to `MAX_PDF_PAGES` pages.
/// Scanned PDFs or failures return `NoContent`.
pub fn extract_pdf_text(data: &[u8]) -> Result<String> {
    let doc = PdfDocument::from_bytes(data.to_vec())
        .map_err(|e| {
            tracing::error!(target: "extract", "pdf_oxide load failed: {e}");
            Error::NoContent
        })?;

    let total = doc.page_count().map_err(|e| {
        tracing::error!(target: "extract", "pdf_oxide page_count failed: {e}");
        Error::NoContent
    })?;

    let take = total.min(MAX_PDF_PAGES);
    let mut text = String::new();
    for i in 0..take {
        match doc.extract_text(i) {
            Ok(page_text) => {
                if !text.is_empty() && !page_text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&page_text);
            }
            Err(e) => {
                tracing::debug!(target: "extract", "pdf_oxide page {i} failed: {e}");
            }
        }
    }

    if total > MAX_PDF_PAGES {
        tracing::info!(target: "extract", "pdf_pages={} limited_to={}", total, MAX_PDF_PAGES);
    }

    if text.trim().is_empty() {
        return Err(Error::NoContent);
    }
    Ok(text)
}
