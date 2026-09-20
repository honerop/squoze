use std::path::Path;

use crate::error::{ArchiveError, Result};
use crate::formats::{self, ArchiveFormat};
use crate::preview;
use crate::util::ensure_parent;

pub struct StandardBackend;

impl StandardBackend {
    pub fn create(input: &Path, output: &Path) -> Result<()> {
        if !input.exists() {
            return Err(crate::error::ArchiveError::InvalidPath(
                input.display().to_string(),
            ));
        }

        ensure_parent(output)?;

        match formats::archive_format(output)? {
            ArchiveFormat::Zip => formats::zip::create(input, output)?,
            ArchiveFormat::Tar => formats::tar::create(input, output)?,
            ArchiveFormat::Gzip => formats::gzip::create(input, output)?,
            ArchiveFormat::TarGzip => formats::targz::create(input, output)?,
            ArchiveFormat::Zstd => formats::zstd::create(input, output)?,
            ArchiveFormat::TarZstd => formats::tarzst::create(input, output)?,
            ArchiveFormat::SevenZip => formats::sevenz::create(input, output)?,
        }

        Ok(())
    }

    pub fn extract(input: &Path, output: &Path) -> Result<()> {
        if !input.exists() {
            return Err(crate::error::ArchiveError::InvalidPath(
                input.display().to_string(),
            ));
        }

        match formats::archive_format(input)? {
            ArchiveFormat::Zip => formats::zip::extract(input, output)?,
            ArchiveFormat::Tar => formats::tar::extract(input, output)?,
            ArchiveFormat::Gzip => formats::gzip::extract(input, output)?,
            ArchiveFormat::TarGzip => formats::targz::extract(input, output)?,
            ArchiveFormat::Zstd => formats::zstd::extract(input, output)?,
            ArchiveFormat::TarZstd => formats::tarzst::extract(input, output)?,
            ArchiveFormat::SevenZip => formats::sevenz::extract(input, output)?,
        }

        Ok(())
    }

    pub fn preview(input: &Path) -> Result<()> {
        let format = formats::archive_format(input)?;

        let (archive_entries, read_content) = match format {
            ArchiveFormat::Zip => (
                formats::zip::preview(input)?,
                formats::zip::make_fetcher(input),
            ),
            ArchiveFormat::Tar => (
                formats::tar::preview(input)?,
                formats::tar::make_fetcher(input.into(), |file| {
                    Ok(Box::new(file) as Box<dyn std::io::Read>)
                }),
            ),
            ArchiveFormat::Gzip => (
                formats::gzip::preview(input)?,
                formats::gzip::make_fetcher(input.into()),
            ),
            ArchiveFormat::TarGzip => (
                formats::targz::preview(input)?,
                formats::targz::make_fetcher(input.into()),
            ),
            ArchiveFormat::Zstd => (
                formats::zstd::preview(input)?,
                formats::zstd::make_fetcher(input.into()),
            ),
            ArchiveFormat::TarZstd => (
                formats::tarzst::preview(input)?,
                formats::tarzst::make_fetcher(input.into()),
            ),
            ArchiveFormat::SevenZip => (
                formats::sevenz::preview(input)?,
                formats::sevenz::make_fetcher(input.into()),
            ),
        };

        preview::run_preview(archive_entries, read_content)?;

        Ok(())
    }
    pub fn preview_json(input: &Path) -> Result<()> {
        let format = formats::archive_format(input)?;
        let archive_entries = match format {
            ArchiveFormat::Zip => (formats::zip::preview(input)?,),
            ArchiveFormat::Tar => (formats::tar::preview(input)?,),
            ArchiveFormat::Gzip => (formats::gzip::preview(input)?,),
            ArchiveFormat::TarGzip => (formats::targz::preview(input)?,),
            ArchiveFormat::Zstd => (formats::zstd::preview(input)?,),
            ArchiveFormat::TarZstd => (formats::tarzst::preview(input)?,),
            ArchiveFormat::SevenZip => (formats::sevenz::preview(input)?,),
        };
        let json_string = serde_json::to_string(&archive_entries)
            .map_err(|e| ArchiveError::Serialization(e.to_string()))?;
        print!("{json_string}");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::ArchiveEntry;
    use crate::formats::tar;
    use crate::formats::{archive_format, gzip, sevenz, targz, tarzst, zip, zstd};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn fixture(ext: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("project");

        fs::create_dir_all(input.join("src/parser")).unwrap();
        fs::create_dir_all(input.join("docs/guides")).unwrap();
        fs::write(input.join("README.md"), "readme").unwrap();
        fs::write(input.join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(input.join("src/parser/lexer.rs"), "lexer").unwrap();
        fs::write(input.join("docs/guides/intro.md"), "intro").unwrap();

        let output = dir.path().join(format!("project{ext}"));

        StandardBackend::create(&input, &output).unwrap();

        (dir, output)
    }

    fn sorted_names(entries: &[ArchiveEntry]) -> Vec<String> {
        let mut names: Vec<String> = entries
            .iter()
            .map(|e| e.path.to_string_lossy().replace('\\', "/"))
            .collect();
        names.sort();
        names
    }

    fn preview_of(fx: &PathBuf) -> Vec<ArchiveEntry> {
        match archive_format(fx).unwrap() {
            ArchiveFormat::Zip => zip::preview(fx).unwrap(),
            ArchiveFormat::Tar => tar::preview(fx).unwrap(),
            ArchiveFormat::TarGzip => targz::preview(fx).unwrap(),
            ArchiveFormat::TarZstd => tarzst::preview(fx).unwrap(),
            ArchiveFormat::SevenZip => sevenz::preview(fx).unwrap(),
            other => panic!("unsupported test format: {other:?}"),
        }
    }

    fn assert_roundtrip(ext: &str) {
        let (_dir, output) = fixture(ext);

        let names = sorted_names(&preview_of(&output));

        for expected in [
            "project/README.md",
            "project/src/main.rs",
            "project/src/parser/lexer.rs",
            "project/docs/guides/intro.md",
        ] {
            assert!(
                names.iter().any(|n| n.contains(expected)),
                "{ext}: missing {expected} in {names:?}"
            );
        }

        let out = output.parent().unwrap().join("extracted");
        StandardBackend::extract(&output, &out).unwrap();

        assert_eq!(
            fs::read_to_string(out.join("project/README.md")).unwrap(),
            "readme"
        );
        assert_eq!(
            fs::read_to_string(out.join("project/src/parser/lexer.rs")).unwrap(),
            "lexer"
        );
        assert_eq!(
            fs::read_to_string(out.join("project/docs/guides/intro.md")).unwrap(),
            "intro"
        );
    }

    #[test]
    fn zip_roundtrip() {
        assert_roundtrip(".zip");
    }

    #[test]
    fn tar_roundtrip() {
        assert_roundtrip(".tar");
    }

    #[test]
    fn tar_gz_roundtrip() {
        assert_roundtrip(".tar.gz");
    }

    #[test]
    fn tar_zst_roundtrip() {
        assert_roundtrip(".tar.zst");
    }

    #[test]
    fn sevenz_roundtrip() {
        assert_roundtrip(".7z");
    }

    #[test]
    fn zip_preview_reports_dirs() {
        let (_dir, output) = fixture(".zip");

        let entries = zip::preview(&output).unwrap();

        assert!(
            entries.iter().filter(|e| e.is_dir).count() >= 5,
            "expected dir entries, got: {:?}",
            sorted_names(&entries)
        );
    }

    #[test]
    fn gzip_roundtrip_and_size() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("file.txt");
        fs::write(&input, vec![b'x'; 4321]).unwrap();

        let output = dir.path().join("file.txt.gz");
        StandardBackend::create(&input, &output).unwrap();

        let entries = gzip::preview(&output).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, PathBuf::from("file.txt"));
        assert_eq!(entries[0].size, 4321);

        let out = dir.path().join("out");
        StandardBackend::extract(&output, &out).unwrap();

        assert_eq!(fs::read(&out).unwrap(), vec![b'x'; 4321]);
    }

    #[test]
    fn zstd_roundtrip_and_size() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("file.txt");
        fs::write(&input, vec![b'y'; 70000]).unwrap();

        let output = dir.path().join("file.zst");
        StandardBackend::create(&input, &output).unwrap();

        let entries = zstd::preview(&output).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].size, 70000);

        let out = dir.path().join("out");
        StandardBackend::extract(&output, &out).unwrap();

        assert_eq!(fs::read(&out).unwrap(), vec![b'y'; 70000]);
    }

    #[test]
    fn rejects_missing_input() {
        let dir = TempDir::new().unwrap();

        let zip = dir.path().join("out.zip");
        assert!(StandardBackend::create(&dir.path().join("nope"), &zip).is_err());
        assert!(StandardBackend::extract(&dir.path().join("nope.zip"), &zip).is_err());

        assert!(StandardBackend::preview(&dir.path().join("nope.7z")).is_err());
    }

    #[test]
    fn rejects_unsupported_format() {
        let dir = TempDir::new().unwrap();

        assert!(matches!(
            archive_format(&dir.path().join("x.rar")).unwrap_err(),
            ArchiveError::UnsupportedFormat(_)
        ));
    }

    #[test]
    fn format_detection() {
        let dir = TempDir::new().unwrap();

        let p = |name: &str| dir.path().join(name);

        assert!(matches!(
            archive_format(&p("a.zip")),
            Ok(ArchiveFormat::Zip)
        ));
        assert!(matches!(
            archive_format(&p("a.tar")),
            Ok(ArchiveFormat::Tar)
        ));
        assert!(matches!(
            archive_format(&p("a.tar.gz")),
            Ok(ArchiveFormat::TarGzip)
        ));
        assert!(matches!(
            archive_format(&p("a.tgz")),
            Ok(ArchiveFormat::TarGzip)
        ));
        assert!(matches!(
            archive_format(&p("a.tar.zst")),
            Ok(ArchiveFormat::TarZstd)
        ));
        assert!(matches!(
            archive_format(&p("a.gz")),
            Ok(ArchiveFormat::Gzip)
        ));
        assert!(matches!(
            archive_format(&p("a.zst")),
            Ok(ArchiveFormat::Zstd)
        ));
        assert!(matches!(
            archive_format(&p("a.7z")),
            Ok(ArchiveFormat::SevenZip)
        ));
    }
}

#[cfg(test)]
mod fetcher_tests {
    use super::*;
    use crate::entry::ContentFetcher;
    use crate::formats::{archive_format, gzip, sevenz, tar, targz, tarzst, zip, zstd};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn fetchers_read_entries() {
        for ext in [".zip", ".tar", ".tar.gz", ".tar.zst", ".7z"] {
            let dir = TempDir::new().unwrap();
            let input = dir.path().join("project");

            fs::create_dir_all(input.join("src/parser")).unwrap();
            fs::write(input.join("README.md"), "readme").unwrap();
            fs::write(input.join("src/main.rs"), "fn main() {}").unwrap();

            let output = dir.path().join(format!("project{ext}"));
            StandardBackend::create(&input, &output).unwrap();

            let fetcher: ContentFetcher =
                match archive_format(&output).unwrap() {
                    ArchiveFormat::Zip => zip::make_fetcher(&output),
                    ArchiveFormat::Tar => tar::make_fetcher(output.clone(), |f| {
                        Ok(Box::new(f) as Box<dyn std::io::Read>)
                    }),
                    ArchiveFormat::TarGzip => targz::make_fetcher(output.clone()),
                    ArchiveFormat::TarZstd => tarzst::make_fetcher(output.clone()),
                    ArchiveFormat::SevenZip => sevenz::make_fetcher(output.clone()),
                    other => panic!("unexpected: {other:?}"),
                };

            assert_eq!(
                fetcher(&Path::new("project/README.md")).unwrap(),
                b"readme".to_vec(),
                "{ext}"
            );
            assert_eq!(
                fetcher(&Path::new("project/src/main.rs")).unwrap(),
                b"fn main() {}".to_vec(),
                "{ext}"
            );
            assert!(fetcher(&Path::new("project/src/")).is_err(), "{ext}");
            assert!(fetcher(&Path::new("no/such/file")).is_err(), "{ext}");
        }
    }

    #[test]
    fn single_file_fetchers() {
        let dir = TempDir::new().unwrap();

        let txt = dir.path().join("note.txt");
        fs::write(&txt, "hello").unwrap();

        let gz = dir.path().join("note.txt.gz");
        StandardBackend::create(&txt, &gz).unwrap();
        assert_eq!(
            gzip::make_fetcher(gz.clone())(&Path::new("whatever")).unwrap(),
            b"hello".to_vec()
        );

        let zst = dir.path().join("note.zst");
        StandardBackend::create(&txt, &zst).unwrap();
        assert_eq!(
            zstd::make_fetcher(zst.clone())(&Path::new("whatever")).unwrap(),
            b"hello".to_vec()
        );
    }
}
