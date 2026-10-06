import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { Bootstrap, Highlight, LibrarySnapshot, Position, ReadingDocument, Settings, SyncProgress, ViewsConfig } from './types';

export const bootstrap = () => invoke<Bootstrap>('bootstrap');
export const connect = (token: string) => invoke<Bootstrap>('connect', { token });
export const disconnect = () => invoke<void>('disconnect');
export const syncLibrary = (full = false) => invoke<LibrarySnapshot>('sync_library', { full });
export const readDocument = (id: string, chapter: number | null = null) => invoke<ReadingDocument>('read_document', { id, chapter });
export const archiveDocument = (id: string) => invoke<void>('archive_document', { id });
export const createHighlight = (id: string, text: string, offset: number | null, chapter: number | null) => invoke<Highlight>('create_highlight', { id, text, offset, chapter });
export const savePosition = (id: string, position: Position) => invoke<void>('save_position', { id, position });
export const saveSettings = (settings: Settings) => invoke<void>('save_settings', { settings });
export const reloadViews = () => invoke<ViewsConfig>('reload_views');
export const openConfig = () => invoke<void>('open_config');
export const openExternal = (url: string) => invoke<void>('open_external', { url });
export const onSyncProgress = (callback: (progress: SyncProgress) => void) => listen<SyncProgress>('sync-progress', event => callback(event.payload));
