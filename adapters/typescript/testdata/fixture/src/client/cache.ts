// A client that keeps the books it reads, so the web side asks the service
// less often.

import type { Book, BookID } from "../domain/book.js";
import { type Fetch, HttpClient } from "./api.js";

/** Settings and types of the cache. */
export namespace Cache {
  /** How long a cached book stays fresh, in milliseconds. */
  export const ttl = 60_000;
  /** A cached book and the time it arrived. */
  export interface Entry {
    book: Book;
    at: number;
  }
}

/** CachingClient keeps each book it reads for {@link Cache.ttl} milliseconds. */
export class CachingClient extends HttpClient {
  private readonly books = new Map<BookID, Cache.Entry>();

  constructor(
    base: string,
    fetch: Fetch,
    private readonly now: () => number = Date.now,
  ) {
    super(base, fetch);
  }

  /** Reads a book from the cache, or from the service when the cache holds none. */
  override async getBook(id: BookID): Promise<Book> {
    const hit = this.books.get(id);
    if (hit && this.now() - hit.at < Cache.ttl) return hit.book;
    const book = await super.getBook(id);
    this.books.set(id, { book, at: this.now() });
    return book;
  }

  override describe(): string {
    return "CachingClient(" + super.describe() + ")";
  }
}

/** Yields the IDs of the cached books, oldest first. */
export function* cachedIds(c: { keys(): Iterable<BookID> }): Generator<BookID> {
  yield* c.keys();
}
