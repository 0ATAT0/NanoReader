import type { Document, Settings } from './types';

export function readingMinutes(document: Document): number | null {
  if (!document.reading_time) return null;
  const value = document.reading_time.trim();
  const minutes = /^(\d+(?:\.\d+)?)\s*(?:mins?|minutes?)$/i.exec(value);
  if (minutes) return Number(minutes[1]);
  const hours = /^(\d+(?:\.\d+)?)\s*(?:hrs?|hours?)(?:\s+(\d+)\s*(?:mins?|minutes?))?$/i.exec(value);
  return hours ? Number(hours[1]) * 60 + Number(hours[2] ?? 0) : null;
}

type Predicate = (document: Document) => boolean;
const locations: Record<string, string> = { inbox: 'new', new: 'new', later: 'later', shortlist: 'shortlist', archive: 'archive' };

export function builtInMatch(document: Document, key: string): boolean {
  if (key === 'books') return document.category === 'epub' && ['new', 'later', 'shortlist'].includes(document.location);
  return document.location === (key === 'inbox' ? 'new' : key) && !['highlight', 'note'].includes(document.category);
}

// Compile once per configured view; the per-document path has no parsing or allocation.
export function compileQuery(query: string): Predicate {
  if (!query.trim() || query.length > 2048) throw new Error('Use a query of 1–2048 characters.');
  const tokens: string[] = [];
  const lexer = /\s*(\(|\)|(?:[^\s()"']|"[^"\\]*(?:\\.[^"\\]*)*"|'[^'\\]*(?:\\.[^'\\]*)*')+)/gy;
  let end = 0;
  while (end < query.length) {
    if (!query.slice(end).trim()) break;
    lexer.lastIndex = end;
    const match = lexer.exec(query);
    if (!match) throw new Error(`Cannot read query near '${query.slice(end, end + 24).trim()}'.`);
    tokens.push(match[1]);
    end = lexer.lastIndex;
  }
  let cursor = 0;
  let depth = 0;
  const peek = () => tokens[cursor]?.toUpperCase();
  const atom = (): Predicate => {
    if (++depth > 20) throw new Error('Queries can have at most 20 nested groups.');
    let predicate: Predicate;
    if (peek() === '(') {
      cursor++;
      predicate = or();
      if (peek() !== ')') throw new Error('A query group is missing its closing parenthesis.');
      cursor++;
    } else {
      const token = tokens[cursor++];
      if (!token || token === ')' || ['AND', 'OR'].includes(token.toUpperCase())) throw new Error('Expected a filter such as in:inbox.');
      predicate = term(token);
    }
    depth--;
    return predicate;
  };
  const and = (): Predicate => {
    let left = atom();
    while (peek() === 'AND') { cursor++; const a = left, b = atom(); left = doc => a(doc) && b(doc); }
    return left;
  };
  const or = (): Predicate => {
    let left = and();
    while (peek() === 'OR') { cursor++; const a = left, b = and(); left = doc => a(doc) || b(doc); }
    return left;
  };
  const predicate = or();
  if (cursor !== tokens.length) throw new Error(`Unexpected '${tokens[cursor]}'; join filters with AND or OR.`);
  return predicate;
}

function term(token: string): Predicate {
  const match = /^([a-z]+)(?:__([a-z]+))?:(.+)$/i.exec(token);
  if (!match) throw new Error(`Unsupported filter '${token}'.`);
  const field = match[1].toLowerCase();
  const operator = match[2]?.toLowerCase() ?? 'eq';
  let value = match[3];
  if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) {
    value = value.slice(1, -1).replace(/\\([\\"'])/g, '$1');
  }
  value = value.toLowerCase();
  if (!value) throw new Error(`Filter '${field}' needs a value.`);
  if (field === 'minutes') {
    const number = Number(value);
    if (!Number.isFinite(number) || number < 0 || !['eq', 'lt', 'lte', 'gt', 'gte'].includes(operator)) throw new Error(`Unsupported reading-time comparison '${token}'.`);
    return doc => {
      const minutes = readingMinutes(doc);
      if (minutes === null) return false;
      switch (operator) {
        case 'lt': return minutes < number;
        case 'lte': return minutes <= number;
        case 'gt': return minutes > number;
        case 'gte': return minutes >= number;
        default: return minutes === number;
      }
    };
  }
  if (!['eq', 'not'].includes(operator)) throw new Error(`Unsupported operator '${operator}' for '${field}'.`);
  let predicate: Predicate;
  switch (field) {
    case 'in': {
      const location = locations[value];
      if (!location) throw new Error(`Unsupported location '${value}'; this app imports saved library items, excluding Feed.`);
      predicate = doc => doc.location === location;
      break;
    }
    case 'category': case 'type':
      if (!['article', 'epub', 'email', 'pdf', 'tweet', 'rss', 'video'].includes(value)) throw new Error(`Unsupported category '${value}'.`);
      predicate = doc => doc.category.toLowerCase() === value;
      break;
    case 'has':
      if (value !== 'tags') throw new Error(`Only has:tags and has__not:tags are supported.`);
      predicate = doc => Object.keys(doc.tags).length > 0;
      break;
    case 'tag':
      predicate = doc => Object.entries(doc.tags).some(([key, tag]) => key.toLowerCase() === value ||
        (typeof tag === 'string' && tag.toLowerCase() === value) ||
        (tag !== null && typeof tag === 'object' && 'name' in tag && typeof tag.name === 'string' && tag.name.toLowerCase() === value));
      break;
    default: throw new Error(`Unsupported field '${field}'; supported fields are in, category, type, tag, has and minutes.`);
  }
  return operator === 'not' ? doc => !predicate(doc) : predicate;
}

export function selectDocuments(documents: Document[], predicate: Predicate, search: string, sort: Settings['sort']): Document[] {
  const needle = search.trim().toLowerCase();
  const result = documents.filter(doc => predicate(doc) && (!needle || [doc.title, doc.author, doc.site_name].some(text => text?.toLowerCase().includes(needle))));
  result.sort((a, b) => {
    if (sort === 'shortest') return (readingMinutes(a) ?? Infinity) - (readingMinutes(b) ?? Infinity) || a.id.localeCompare(b.id);
    const date = (b.saved_at ?? '').localeCompare(a.saved_at ?? '');
    return (sort === 'oldest' ? -date : date) || a.id.localeCompare(b.id);
  });
  return result;
}
