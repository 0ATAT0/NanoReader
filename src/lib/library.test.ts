import { describe, expect, it } from 'vitest';
import { builtInMatch, compileQuery, readingMinutes, selectDocuments } from './library';
import type { Document } from './types';
import examples from '../../views.example.json';

function article(id: string, values: Partial<Document> = {}): Document {
  return { id, url: `https://read.readwise.io/read/${id}`, source_url: 'https://example.com/article',
    title: id, author: null, site_name: null, category: 'article', location: 'new', image_url: null,
    word_count: null, reading_time: '7 mins', reading_progress: 0, saved_at: '2026-01-01T00:00:00Z',
    published_date: null, updated_at: '2026-01-01T00:00:00Z', summary: null, tags: {}, ...values };
}

describe('configured Reader views', () => {
  it('VC-QR-1: copied sample queries respect inclusive Quick boundary, locations, tags and category exclusions', () => {
    const documents = [article('short'), article('ten', { reading_time: '10 mins' }),
      article('long', { location: 'later', reading_time: '22 mins' }),
      article('tagged', { tags: { technical: { name: 'Technical' } } }), article('feed', { location: 'feed' }),
      article('archived', { location: 'archive' }), article('mail', { category: 'email' }),
      article('video', { category: 'video' }), article('book', { category: 'epub', reading_time: '240 mins' }),
      article('unknown', { reading_time: null, word_count: 1000 })];
    expect(documents.filter(compileQuery(examples.views[0].query)).map(doc => doc.id)).toEqual(['short', 'ten']);
    expect(documents.filter(compileQuery(examples.views[1].query)).map(doc => doc.id)).toEqual(['long']);
    expect(documents.filter(doc => builtInMatch(doc, 'books')).map(doc => doc.id)).toEqual(['book']);
    expect(documents.filter(doc => builtInMatch(doc, 'later')).map(doc => doc.id)).toEqual(['long']);
    expect(builtInMatch(article('tagged-book', { category: 'epub', tags: { keep: 'Keep' } }), 'books')).toBe(true);
  });

  it('evaluates Boolean precedence, nested groups, quoted tags and negation without substring matching', () => {
    const predicate = compileQuery('tag:"City Planning" OR (in:later AND type__not:epub)');
    expect(predicate(article('a', { tags: { 'city-planning': { name: 'City Planning' } } }))).toBe(true);
    expect(predicate(article('b', { location: 'later' }))).toBe(true);
    expect(predicate(article('c', { location: 'later', category: 'epub' }))).toBe(false);
    expect(predicate(article('d', { tags: { 'city-planning-notes': 'City Planning Notes' } }))).toBe(false);
    expect(compileQuery('in:inbox OR in:later AND minutes__gt:20')(article('new'))).toBe(true);
    expect(compileQuery('(in:inbox OR in:later) AND minutes__gt:20')(article('new'))).toBe(false);
  });

  it.each(['saved__after:"1 week ago"', 'rssSource:abc', 'minutes__contains:10', 'in:feed', 'has:highlights',
    '(in:inbox', 'in:inbox)', 'in:inbox category:article', 'tag:"unterminated', 'in:later AND'])
    ('reports unsupported or malformed query %s rather than showing approximate results', query => {
      expect(() => compileQuery(query)).toThrow();
    });

  it('sorts and searches returned metadata without mutating the authoritative list or guessing missing time', () => {
    const docs = [article('older', { title: 'Streets for people', author: 'Ada', reading_time: '1 hour 20 mins', saved_at: '2025-01-01' }),
      article('newer', { site_name: 'Urban Review', reading_time: '12 mins', saved_at: '2026-01-01' }), article('unknown', { reading_time: null })];
    expect(readingMinutes(docs[0])).toBe(80);
    expect(readingMinutes(docs[2])).toBeNull();
    expect(selectDocuments(docs, () => true, '', 'shortest').map(doc => doc.id)).toEqual(['newer', 'older', 'unknown']);
    expect(selectDocuments(docs, () => true, 'ADA', 'newest').map(doc => doc.id)).toEqual(['older']);
    expect(selectDocuments(docs, () => true, 'urban', 'newest').map(doc => doc.id)).toEqual(['newer']);
    expect(docs.map(doc => doc.id)).toEqual(['older', 'newer', 'unknown']);
  });
});
