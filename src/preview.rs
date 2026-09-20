use crate::entry::{ArchiveEntry, ContentFetcher};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use std::{io, path::Path, path::PathBuf};

struct Node {
    id: usize,
    name: String,
    path: PathBuf,
    is_dir: bool,
    expanded: bool,
    size: u64,
    children: Vec<Node>,
}

impl Node {
    fn new(name: impl Into<String>) -> Self {
        Self {
            id: 0,
            name: name.into(),
            path: PathBuf::new(),
            is_dir: false,
            expanded: false,
            size: 0,
            children: Vec::new(),
        }
    }
}

struct Row {
    prefix: String,
    id: usize,
    name: String,
    path: PathBuf,
    is_dir: bool,
    expanded: bool,
    size: u64,
}

struct ViewState {
    title: String,
    lines: Vec<Line<'static>>,
    max_scroll: u16,
    scroll: u16,
}

const MAX_VIEW_BYTES: usize = 1024 * 1024;

fn build_view(path: &Path, mut data: Vec<u8>) -> ViewState {
    let binary = data.contains(&0);

    if data.len() > MAX_VIEW_BYTES {
        data.truncate(MAX_VIEW_BYTES);
    }

    let mut lines: Vec<Line<'static>> = if binary {
        vec![Line::styled(
            format!("(binary file, {})", human_size(data.len() as u64)),
            Style::new().dark_gray().italic(),
        )]
    } else if data.is_empty() {
        vec![Line::styled(
            "(empty file)",
            Style::new().dark_gray().italic(),
        )]
    } else {
        String::from_utf8_lossy(&data)
            .lines()
            .map(|l| Line::from(l.to_string()))
            .collect()
    };

    if data.len() == MAX_VIEW_BYTES {
        lines.push(Line::styled(
            format!("(truncated at {})", human_size(MAX_VIEW_BYTES as u64)),
            Style::new().dark_gray().italic(),
        ));
    }

    let max_scroll = lines.len().saturating_sub(1).min(u16::MAX as usize) as u16;

    ViewState {
        title: format!(" {} — ↑/↓ scroll, Esc back ", path.display()),
        lines,
        max_scroll,
        scroll: 0,
    }
}

fn build_tree(files: &[ArchiveEntry]) -> Node {
    let mut root = Node::new("");

    for file in files {
        let raw = file.path.to_string_lossy().replace('\\', "/");
        let parts: Vec<&str> = raw
            .split('/')
            .filter(|p| !p.is_empty() && *p != "." && *p != "..")
            .collect();

        let mut node = &mut root;

        for (i, part) in parts.iter().enumerate() {
            let last = i == parts.len() - 1;

            let idx = match node.children.iter().position(|c| c.name == *part) {
                Some(idx) => idx,
                None => {
                    let mut child = Node::new(*part);
                    child.path = node.path.join(part);
                    node.children.push(child);
                    node.children.len() - 1
                }
            };

            node = &mut node.children[idx];

            if last {
                if !file.is_dir {
                    node.is_dir = false;
                    node.size = file.size;
                } else {
                    node.is_dir = true;
                }
            } else {
                node.is_dir = true;
            }
        }
    }

    aggregate_sizes(&mut root);
    sort_tree(&mut root);
    assign_ids(&mut root, &mut 0);

    root
}

fn aggregate_sizes(node: &mut Node) -> u64 {
    let total: u64 = node.children.iter_mut().map(aggregate_sizes).sum();

    if node.is_dir {
        node.size = total;
    }

    node.size
}

fn sort_tree(node: &mut Node) {
    node.children.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    for child in &mut node.children {
        sort_tree(child);
    }
}

fn assign_ids(node: &mut Node, next: &mut usize) {
    node.id = *next;
    *next += 1;

    for child in &mut node.children {
        assign_ids(child, next);
    }
}

fn count_entries(node: &Node) -> (usize, usize) {
    let mut dirs = 0;
    let mut files = 0;

    for child in &node.children {
        if child.is_dir {
            dirs += 1;
            let (d, f) = count_entries(child);
            dirs += d;
            files += f;
        } else {
            files += 1;
        }
    }

    (dirs, files)
}

fn collect_visible(node: &Node, prefix: &str, out: &mut Vec<Row>) {
    for (i, child) in node.children.iter().enumerate() {
        let last = i == node.children.len() - 1;

        let branch = if last { "└── " } else { "├── " };

        out.push(Row {
            prefix: format!("{prefix}{branch}"),
            id: child.id,
            name: child.name.clone(),
            path: child.path.clone(),
            is_dir: child.is_dir,
            expanded: child.expanded,
            size: child.size,
        });

        if child.is_dir && child.expanded {
            let child_prefix = format!("{prefix}{}", if last { "    " } else { "│   " });
            collect_visible(child, &child_prefix, out);
        }
    }
}

enum Toggle {
    Expand,
    Collapse,
    Flip,
}

fn toggle_node(node: &mut Node, id: usize, action: &Toggle) -> bool {
    if node.id == id {
        if node.is_dir {
            match action {
                Toggle::Expand => node.expanded = true,
                Toggle::Collapse => node.expanded = false,
                Toggle::Flip => node.expanded = !node.expanded,
            }
        }

        return true;
    }

    node.children
        .iter_mut()
        .any(|child| toggle_node(child, id, action))
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];

    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn row_to_item(row: &Row) -> ListItem<'static> {
    let mut spans = vec![Span::styled(row.prefix.clone(), Style::new().dim())];

    if row.is_dir {
        spans.push(Span::styled(
            if row.expanded { "▾ " } else { "▸ " },
            Style::new().cyan(),
        ));

        spans.push(Span::styled(
            format!("{}/", row.name),
            Style::new().cyan().bold(),
        ));
    } else {
        spans.push(Span::raw("  "));
        spans.push(Span::raw(row.name.clone()));
    }

    spans.push(Span::styled(
        format!("  {}", human_size(row.size)),
        Style::new().dark_gray(),
    ));

    ListItem::new(Line::from(spans))
}

fn visible_rows(root: &Node) -> Vec<Row> {
    let mut rows = Vec::new();
    collect_visible(root, "", &mut rows);
    rows
}

fn refresh_rows(root: &Node) -> (Vec<Row>, Vec<ListItem<'static>>) {
    let rows = visible_rows(root);
    let items = make_items(&rows);
    (rows, items)
}

fn make_items(rows: &[Row]) -> Vec<ListItem<'static>> {
    if rows.is_empty() {
        return vec![ListItem::new(Line::styled(
            "(empty archive)",
            Style::new().dark_gray().italic(),
        ))];
    }

    rows.iter().map(row_to_item).collect()
}

fn draw_screen(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    view: Option<&ViewState>,
    items: &[ListItem<'static>],
    title: &str,
    sel: usize,
) -> io::Result<()> {
    terminal.draw(|frame| {
        if let Some(view) = view {
            let paragraph = Paragraph::new(view.lines.clone())
                .block(
                    Block::default()
                        .title(view.title.clone())
                        .borders(Borders::ALL),
                )
                .scroll((view.scroll, 0));

            frame.render_widget(paragraph, frame.area());
        } else {
            let list = List::new(items.to_vec())
                .block(
                    Block::default()
                        .title(title.to_string())
                        .borders(Borders::ALL),
                )
                .highlight_style(Style::new().bg(Color::DarkGray));

            let mut state = ListState::default();

            if !items.is_empty() {
                state.select(Some(sel));
            }

            frame.render_stateful_widget(list, frame.area(), &mut state);
        }
    })?;

    Ok(())
}

pub fn run_preview(files: Vec<ArchiveEntry>, read_content: ContentFetcher) -> io::Result<()> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();

    execute!(stdout, EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);

    let mut terminal = Terminal::new(backend)?;

    let mut root = build_tree(&files);

    let (dirs, files_count) = count_entries(&root);

    let title = format!(
        " Archive Preview — {dirs} dirs, {files_count} files — Enter: open, ↑/↓ move, q quit "
    );

    let mut sel: usize = 0;

    let (mut rows, mut items) = refresh_rows(&root);

    let mut view: Option<ViewState> = None;

    draw_screen(&mut terminal, view.as_ref(), &items, &title, sel)?;

    loop {
        match event::read()? {
            Event::Key(key) => {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                let mut redraw = false;

                if view.is_some() {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc | KeyCode::Left => {
                            view = None;
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if let Some(v) = view.as_mut() {
                                v.scroll = (v.scroll + 1).min(v.max_scroll);
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if let Some(v) = view.as_mut() {
                                v.scroll = v.scroll.saturating_sub(1);
                            }
                        }
                        KeyCode::PageDown => {
                            let step = terminal_size_height(&mut terminal) as u16;
                            if let Some(v) = view.as_mut() {
                                v.scroll = (v.scroll + step).min(v.max_scroll);
                            }
                        }
                        KeyCode::PageUp => {
                            let step = terminal_size_height(&mut terminal) as u16;
                            if let Some(v) = view.as_mut() {
                                v.scroll = v.scroll.saturating_sub(step);
                            }
                        }
                        KeyCode::Home => {
                            if let Some(v) = view.as_mut() {
                                v.scroll = 0;
                            }
                        }
                        KeyCode::End => {
                            if let Some(v) = view.as_mut() {
                                v.scroll = v.max_scroll;
                            }
                        }
                        _ => {}
                    }

                    redraw = true;
                } else {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Down | KeyCode::Char('j') => {
                            if !rows.is_empty() {
                                sel = (sel + 1).min(rows.len() - 1);
                            }
                            redraw = true;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            sel = sel.saturating_sub(1);
                            redraw = true;
                        }
                        KeyCode::PageDown => {
                            if !rows.is_empty() {
                                let step = terminal_size_height(&mut terminal);
                                sel = (sel + step).min(rows.len() - 1);
                            }
                            redraw = true;
                        }
                        KeyCode::PageUp => {
                            let step = terminal_size_height(&mut terminal);
                            sel = sel.saturating_sub(step);
                            redraw = true;
                        }
                        KeyCode::Home => {
                            sel = 0;
                            redraw = true;
                        }
                        KeyCode::End => {
                            sel = rows.len().saturating_sub(1);
                            redraw = true;
                        }
                        KeyCode::Enter => {
                            if let Some(row) = rows.get(sel) {
                                if row.is_dir {
                                    toggle_node(&mut root, row.id, &Toggle::Flip);
                                    (rows, items) = refresh_rows(&root);
                                    sel = sel.min(rows.len().saturating_sub(1));
                                } else {
                                    let path = row.path.clone();

                                    if let Ok(data) = read_content(&path) {
                                        view = Some(build_view(&path, data));
                                    }
                                }
                                redraw = true;
                            }
                        }
                        KeyCode::Right => {
                            if let Some(row) = rows.get(sel) {
                                if row.is_dir && !row.expanded {
                                    toggle_node(&mut root, row.id, &Toggle::Expand);
                                    (rows, items) = refresh_rows(&root);
                                    sel = sel.min(rows.len().saturating_sub(1));
                                    redraw = true;
                                }
                            }
                        }
                        KeyCode::Left => {
                            if let Some(row) = rows.get(sel) {
                                if row.is_dir && row.expanded {
                                    toggle_node(&mut root, row.id, &Toggle::Collapse);
                                    (rows, items) = refresh_rows(&root);
                                    sel = sel.min(rows.len().saturating_sub(1));
                                    redraw = true;
                                }
                            }
                        }
                        _ => {}
                    }
                }

                if redraw {
                    draw_screen(&mut terminal, view.as_ref(), &items, &title, sel)?;
                }
            }
            Event::Resize(_, _) => {
                draw_screen(&mut terminal, view.as_ref(), &items, &title, sel)?;
            }
            _ => {}
        }
    }

    disable_raw_mode()?;

    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    terminal.show_cursor()?;

    Ok(())
}

fn terminal_size_height(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> usize {
    terminal
        .size()
        .map(|area| area.height as usize)
        .unwrap_or(10)
}
