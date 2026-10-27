//! Media link commands: add and remove links as plans on `links.json`, so they share the
//! project history and undo with every other change.

use crate::{Origin, Plan, Result, apply, read, version};
use spearfia_core::links::{LINKS_FILE, Link, Links};
use spearfia_core::project::Project;

fn load(project: &Project) -> Result<(String, Links)> {
    let text = read(project, LINKS_FILE)?;
    let links = Links::parse(&text).map_err(|e| {
        crate::OpError::Invalid(format!(
            "{LINKS_FILE} can't be read ({e}); fix or restore it before changing links."
        ))
    })?;
    Ok((text, links))
}

fn save(
    project: &Project,
    before: String,
    links: &Links,
    summary: String,
    origin: Origin,
) -> Result<()> {
    let text = links
        .to_text()
        .map_err(|e| crate::OpError::Invalid(e.to_string()))?;
    let plan = Plan {
        doc: LINKS_FILE.into(),
        base: version(&before),
        before,
        text,
        summary,
    };
    apply(project, &plan, origin)?;
    Ok(())
}

/// Add links starting from the saved file, and return the saved links and the new (or
/// existing identical) ids.
pub fn add(project: &Project, requests: Vec<Link>, origin: Origin) -> Result<(Links, Vec<String>)> {
    let (before, mut links) = load(project)?;
    let count = requests.len();
    let ids = links.add_requests(requests)?;
    let summary = match count {
        1 => "Link 1 clip".to_owned(),
        n => format!("Link {n} clips"),
    };
    save(project, before, &links, summary, origin)?;
    Ok((links, ids))
}

/// Remove one link, starting from the saved file. Returns the saved links.
pub fn remove(project: &Project, id: &str, origin: Origin) -> Result<Links> {
    let (before, mut links) = load(project)?;
    let link = links
        .remove(id)
        .ok_or_else(|| crate::OpError::NotFound(format!("No media link {id}.")))?;
    save(
        project,
        before,
        &links,
        format!("Unlink {} from \"{}\"", link.media.path, link.quote),
        origin,
    )?;
    Ok(links)
}

/// Replace only this job's previous links and register its verified outputs together.
pub fn replace(
    project: &Project,
    old: &[String],
    requests: Vec<Link>,
    origin: Origin,
) -> Result<(Links, Vec<String>)> {
    let (before, mut links) = load(project)?;
    for id in old {
        links.remove(id);
    }
    let ids = links.add_requests(requests)?;
    save(
        project,
        before,
        &links,
        "Register verified production artifacts".into(),
        origin,
    )?;
    Ok((links, ids))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::project;
    use spearfia_core::links::{LinkKind, LinkMedia};
    use spearfia_core::project::MediaRole;

    fn link(path: &str) -> Link {
        Link {
            id: String::new(),
            quote: "hello".into(),
            line: 0,
            media: LinkMedia {
                path: path.into(),
                hash: None,
            },
            role: MediaRole::BRoll,
            kind: LinkKind::Picture,
            in_point: None,
            out_point: None,
        }
    }

    #[test]
    fn links_start_from_disk_and_undo_with_the_shared_history() {
        let p = project("links", "VO: hello\n");
        // A stale copy (e.g. the desktop's) never matters: every change starts from disk.
        let (_, a) = add(&p, vec![link("a.mp4")], Origin::Agent).unwrap();
        let (saved, b) = add(&p, vec![link("b.mp4")], Origin::App).unwrap();
        assert_eq!((a, b), (vec!["k1".to_owned()], vec!["k2".to_owned()]));
        assert_eq!(Links::load(&p).unwrap(), saved);
        // Adding an identical link changes nothing and records nothing.
        add(&p, vec![link("a.mp4")], Origin::Agent).unwrap();
        assert_eq!(crate::history(&p).unwrap().len(), 2);

        remove(&p, "k1", Origin::Agent).unwrap();
        assert!(remove(&p, "k1", Origin::Agent).is_err());
        assert_eq!(
            crate::undo(&p).unwrap().summary,
            "Unlink a.mp4 from \"hello\""
        );
        assert_eq!(Links::load(&p).unwrap().links.len(), 2);
        crate::undo(&p).unwrap();
        crate::undo(&p).unwrap();
        assert!(Links::load(&p).unwrap().links.is_empty());
        crate::redo(&p).unwrap();
        assert_eq!(Links::load(&p).unwrap().links.len(), 1);
    }

    #[test]
    fn an_unreadable_file_is_left_alone() {
        let p = project("links-broken", "VO: hello\n");
        std::fs::write(p.root.join(LINKS_FILE), "{broken").unwrap();
        assert!(add(&p, vec![link("a.mp4")], Origin::Agent).is_err());
        assert_eq!(
            std::fs::read_to_string(p.root.join(LINKS_FILE)).unwrap(),
            "{broken"
        );
    }
}
