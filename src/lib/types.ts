export interface Document {
  id: string;
  url: string;
  source_url: string | null;
  title: string;
  author: string | null;
  site_name: string | null;
  category: string;
  location: string;
  image_url: string | null;
  word_count: number | null;
  reading_time: string | null;
  reading_progress: number;
  saved_at: string | null;
  published_date: string | null;
  updated_at: string;
  summary: string | null;
  tags: Record<string, unknown>;
}

export interface Settings {
  font_size: number;
  line_height: number;
  reading_width: number;
  view: 'covers' | 'list';
  sort: 'newest' | 'oldest' | 'shortest';
}

export interface Position {
  chapter: number;
  anchor: string;
  offset: number;
  progress: number;
  reader_progress: number;
}

export interface Highlight {
  id: string;
  text: string;
  offset: number | null;
  chapter: number | null;
}

export interface Chapter { index: number; title: string }

export interface ReadingDocument {
  document: Document;
  html: string;
  highlights: Highlight[];
  position: Position | null;
  chapters: Chapter[];
  chapter: number;
}

export interface LibrarySnapshot {
  documents: Document[];
  last_synced: string | null;
}

export interface Bootstrap extends LibrarySnapshot {
  connected: boolean;
  settings: Settings;
  views: CustomView[];
  config_path: string;
  config_error: string | null;
}

export interface SyncProgress { message: string; completed: number }

export interface CustomView { name: string; query: string }
export interface ViewsConfig {
  views: CustomView[];
  config_path: string;
  config_error: string | null;
}
