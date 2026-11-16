/* Helpers that know nothing about this app's screens. */

const MARKUP = Symbol('markup');

const entities: Record<string, string> = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#39;',
};

const escape = (value: string): string =>
  value.replace(/[&<>"']/g, (character) => entities[character] ?? character);

const show = (value: unknown): string => {
  if (value === null || value === undefined || value === false) return '';
  if (typeof value === 'string') return escape(value);
  if (Array.isArray(value)) return value.map(show).join('');
  if (typeof value === 'object' && MARKUP in value) {
    return (value as MarkupNode)[MARKUP];
  }
  return String(value);
};

/** Markup as a value rather than a string in the middle of the logic: interpolated text is escaped,
    markup built by another view is spliced as it is. */
export type Markup = string | MarkupNode | Markup[];

interface MarkupNode {
  readonly [MARKUP]: string;
}

export const html = (strings: TemplateStringsArray, ...values: unknown[]): Markup => {
  let out = strings[0] ?? '';
  values.forEach((value, index) => {
    out += show(value) + (strings[index + 1] ?? '');
  });
  return { [MARKUP]: out };
};

export const asHtml = (content: Markup): string =>
  Array.isArray(content)
    ? content.map(asHtml).join('')
    : typeof content === 'string'
      ? content
      : content[MARKUP];

export const el = <T extends HTMLElement = HTMLElement>(id: string): T =>
  document.getElementById(id) as T;

export const reason = (error: unknown): string => String(error);

export const sleep = (ms: number): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, ms));

/** Bytes with thin spaces, so the counter does not twitch as it fills. */
export const thousands = (value: number | undefined): string =>
  (value ?? 0).toLocaleString('en-US').replace(/,/g, ' ');

export const shortHash = (hash: string | null): string =>
  hash ? `${hash.slice(0, 8)}…${hash.slice(-4)}` : '';

/** Days since the epoch, as the site writes the day. */
export const dayDate = (days: number): string =>
  new Date(days * 86_400_000).toISOString().slice(0, 10);

/** 20261112 as the site writes it. */
export const listDate = (build: number | string): string =>
  `${build}`.replace(/^(\d{4})(\d{2})(\d{2})$/, '$1-$2-$3');
