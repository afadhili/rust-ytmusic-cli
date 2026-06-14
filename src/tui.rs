use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::{
    app::{App, InputMode, PlaylistFocus, Screen},
    types::track::{format_duration, Track},
};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(6),
            Constraint::Length(3),
        ])
        .split(area);

    draw_tabs(frame, app, vertical[0]);
    draw_main(frame, app, vertical[1]);
    draw_now_playing(frame, app, vertical[2]);
    draw_status(frame, app, vertical[3]);
}

fn draw_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let tabs = [
        (Screen::Search, "1 Search"),
        (Screen::Queue, "2 Queue"),
        (Screen::History, "3 History"),
        (Screen::Favorites, "4 Favorites"),
        (Screen::Playlists, "5 Playlists"),
        (Screen::Lyrics, "6 Lyrics"),
    ];

    let spans: Vec<Span> = tabs.iter().flat_map(|(screen, label)| {
        let style = if *screen == app.screen { Style::default().add_modifier(Modifier::REVERSED) } else { Style::default() };
        vec![Span::styled(format!(" {label} "), style), Span::raw(" ")]
    }).collect();

    let paragraph = Paragraph::new(Line::from(spans))
        .block(Block::default().title(" ytmusic-cli ").borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn draw_main(frame: &mut Frame, app: &App, area: Rect) {
    match app.screen {
        Screen::Search => draw_search(frame, app, area),
        Screen::Queue => draw_track_list(frame, " Queue ", app.queue.iter().collect::<Vec<_>>(), app.queue_selected, app.queue_index, area),
        Screen::History => draw_track_list(frame, " History ", app.history.iter().collect::<Vec<_>>(), app.selected, None, area),
        Screen::Favorites => draw_track_list(frame, " Favorites ", app.favorites.iter().collect::<Vec<_>>(), app.selected, None, area),
        Screen::Playlists => draw_playlists(frame, app, area),
        Screen::Lyrics => draw_lyrics(frame, app, area),
    }
}

fn draw_search(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(area);

    let input_title = match app.input_mode {
        InputMode::TypingSearch => " Search input - typing ",
        _ => " Search input - press / ",
    };

    let paragraph = Paragraph::new(app.query.as_str())
        .block(Block::default().title(input_title).borders(Borders::ALL));
    frame.render_widget(paragraph, chunks[0]);

    if app.input_mode == InputMode::TypingSearch {
        frame.set_cursor_position((chunks[0].x + app.query.len() as u16 + 1, chunks[0].y + 1));
    }

    draw_track_list(frame, " Results ", app.results.iter().collect::<Vec<_>>(), app.selected, None, chunks[1]);
}

fn draw_track_list(frame: &mut Frame, title: &str, tracks: Vec<&Track>, selected: usize, playing_index: Option<usize>, area: Rect) {
    let items: Vec<ListItem> = tracks.into_iter().enumerate().map(|(index, track)| {
        let marker = if index == selected { "›" } else { " " };
        let playing = if Some(index) == playing_index { "▶ " } else { "  " };
        let cache = if track.cached_path.is_some() { "cached" } else { "remote" };
        ListItem::new(format!(
            "{marker} {playing}{} - {} [{}] ({cache})",
            track.title,
            track.artist,
            format_duration(track.duration),
        ))
    }).collect();

    let list = List::new(items)
        .block(Block::default().title(title).borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_widget(list, area);
}

fn draw_playlists(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(area);

    let left_title = match app.playlist_focus {
        PlaylistFocus::List => " Playlists [focused] - Tab tracks ",
        PlaylistFocus::Tracks => " Playlists - Tab list ",
    };

    let playlist_items: Vec<ListItem> = app.playlists.iter().enumerate().map(|(i, playlist)| {
        let marker = if i == app.playlist_selected { "›" } else { " " };
        ListItem::new(format!("{marker} {} ({})", playlist.name, playlist.tracks.len()))
    }).collect();

    let left = List::new(playlist_items)
        .block(Block::default().title(left_title).borders(Borders::ALL));
    frame.render_widget(left, chunks[0]);

    if let Some(playlist) = app.playlists.get(app.playlist_selected) {
        let right_title = match app.playlist_focus {
            PlaylistFocus::Tracks => format!(" {} [focused] - Enter play playlist queue ", playlist.name),
            PlaylistFocus::List => format!(" {} ", playlist.name),
        };
        draw_track_list(frame, &right_title, playlist.tracks.iter().collect(), app.playlist_track_selected, None, chunks[1]);
    } else {
        frame.render_widget(Paragraph::new("No playlists. Press P to create one.").block(Block::default().title(" Tracks ").borders(Borders::ALL)), chunks[1]);
    }
}

fn draw_lyrics(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    if let Some(lyrics) = &app.lyrics {
        let kind = if lyrics.instrumental {
            "instrumental"
        } else if lyrics.is_synced() {
            "synced"
        } else {
            "plain"
        };

        lines.push(Line::from(format!(
            "source: {:?} | kind: {} | track: {} - {}",
            lyrics.source, kind, lyrics.track_name, lyrics.artist_name
        )));

        let display_lines = lyrics.display_lines();
        let active = app.active_lyrics_index();
        let height = area.height.saturating_sub(4) as usize;

        let start = if lyrics.is_synced() {
            active
                .map(|index| index.saturating_sub(height / 2))
                .unwrap_or(app.lyrics_scroll)
        } else {
            app.lyrics_scroll
        };

        for (index, line) in display_lines.into_iter().enumerate().skip(start).take(height) {
            let style = if lyrics.is_synced() && Some(index) == active {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };

            lines.push(Line::from(Span::styled(line, style)));
        }
    } else if app.lyrics_loading {
        let track = app
            .now_playing
            .as_ref()
            .map(|track| track.label())
            .unwrap_or_else(|| "current track".to_string());
        lines.push(Line::from(format!("Loading lyrics for {track}...")));
        lines.push(Line::from("TUI remains responsive. You can keep navigating while LRCLIB/YouTube fallback is fetched."));
    } else {
        lines.push(Line::from("No lyrics loaded. Press 6 while a song is playing."));
    }

    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().title(" Lyrics ").borders(Borders::ALL))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn draw_now_playing(frame: &mut Frame, app: &App, area: Rect) {
    let content = if let Some(track) = &app.now_playing {
        vec![
            Line::from(vec![Span::raw("Title : "), Span::raw(track.title.clone())]),
            Line::from(vec![Span::raw("Artist: "), Span::raw(track.artist.clone())]),
            Line::from(vec![Span::raw("Time  : "), Span::raw(app.player_position_label())]),
            Line::from(vec![Span::raw("Volume: "), Span::raw(app.player.volume().to_string())]),
            Line::from(vec![Span::raw("Queue : "), Span::raw(format!("{} track(s), source: {}, index: {}", app.queue.len(), app.queue_source_label(), app.queue_index.map(|i| i + 1).unwrap_or(0)))]),
            Line::from(vec![Span::raw("Source: "), Span::raw(track.cached_path.as_ref().map(|p| p.display().to_string()).unwrap_or_else(|| track.url()))]),
        ]
    } else {
        vec![Line::from("No track playing")]
    };

    frame.render_widget(Paragraph::new(content).block(Block::default().title(" Now Playing ").borders(Borders::ALL)).wrap(Wrap { trim: true }), area);
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let mode = match app.input_mode {
        InputMode::Normal => "NORMAL",
        InputMode::TypingSearch => "SEARCH",
        InputMode::TypingPlaylistName => "PLAYLIST NAME",
    };
    let line = format!(
        "{mode} | {} | / search | 1-6 tabs | Tab playlist focus | Enter play | a queue | r refill | n/b next/prev | f favorite | p playlist | c cache | Space pause | q quit",
        app.status,
    );
    frame.render_widget(Paragraph::new(line).block(Block::default().title(" Status ").borders(Borders::ALL)).wrap(Wrap { trim: true }), area);
}
