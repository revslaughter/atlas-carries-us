// The client for the Go lending API: one interface, and the classes that
// speak HTTP to the service.

import type { Book, BookID, NewBook } from "../domain/book.js";
import { bookPath, type Loan, mayBorrow } from "../domain/loan.js";

/**
 * LendingApi lists what the web side asks of the lending service.
 *
 * @remarks
 * Two classes implement it: {@link HttpClient} and {@link CachingClient}.
 * Code that uses it should not care which one it holds.
 */
export interface LendingApi {
  /** Reads one book. */
  getBook(id: BookID): Promise<Book>;
  /** Lends a book to a member and returns the new loan. */
  borrow(id: BookID, member: string): Promise<Loan>;
  /** Asks whether a member may borrow one more book. */
  canBorrow: (member: string) => Promise<boolean>;
}

/** Fetch is the part of the global fetch function that the client needs. */
export type Fetch = (
  url: string,
  init?: { method?: string; body?: string },
) => Promise<{ ok: boolean; status: number; json(): Promise<unknown> }>;

/**
 * BaseClient holds what every client shares: the service's address and the
 * way to send a request.
 */
export abstract class BaseClient {
  /** How many requests this client has sent. */
  protected sent = 0;

  /**
   * @param base - the address of the Go service, without a trailing slash
   * @param fetch - the function that sends requests
   */
  constructor(
    readonly base: string,
    private readonly fetch: Fetch,
  ) {}

  /**
   * Sends one request and returns the decoded JSON body.
   *
   * @throws {HttpError} when the service answers with an error status
   */
  protected async send<T>(path: string, method = "GET", body?: unknown): Promise<T> {
    this.sent++;
    const res = await this.fetch(this.base + path, {
      method,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (!res.ok) throw new HttpError(res.status, path);
    return (await res.json()) as T;
  }

  /** Describes the client for logs. */
  abstract describe(): string;
}

/** HttpError reports a failed request: its status and its path. */
export class HttpError extends Error {
  constructor(
    readonly status: number,
    readonly path: string,
  ) {
    super(`${path}: status ${status}`);
  }
}

/**
 * HttpClient speaks HTTP to the Go lending service.
 *
 * @example
 * Read one book:
 * ```ts
 * const client = new HttpClient("http://localhost:8080", fetch);
 * const book = await client.getBook("b-1");
 * ```
 */
export class HttpClient extends BaseClient implements LendingApi {
  /** The number of clients made so far. */
  static count = 0;
  #timeout = 5000;

  constructor(base: string, fetch: Fetch) {
    super(base, fetch);
    HttpClient.count++;
  }

  /** The request timeout in milliseconds. It never drops below 100. */
  get timeout(): number {
    return this.#timeout;
  }
  set timeout(ms: number) {
    this.#timeout = Math.max(100, ms);
  }

  getBook(id: BookID): Promise<Book> {
    return this.send<Book>(bookPath(id));
  }

  /**
   * Lends a book. It checks the member's open loans first, as the service
   * does, so that a refusal costs no request.
   *
   * @param id - the book
   * @param member - the member's ID
   */
  async borrow(id: BookID, member: string): Promise<Loan> {
    const loans = await this.loans(member);
    if (!mayBorrow(loans)) throw new HttpError(409, bookPath(id));
    return this.send<Loan>(bookPath(id) + "/loans", "POST", { member });
  }

  canBorrow = async (member: string): Promise<boolean> => mayBorrow(await this.loans(member));

  /** Lists a member's open loans, or every loan with `all`. */
  loans(member: string): Promise<Loan[]>;
  loans(member: string, all: true): Promise<Loan[]>;
  loans(member: string, all?: boolean): Promise<Loan[]> {
    return this.send<Loan[]>(`/api/members/${member}/loans${all ? "?all=1" : ""}`);
  }

  /** Adds a book to the catalogue. */
  add(book: NewBook): Promise<Book> {
    return this.send<Book>("/api/books", "POST", book);
  }

  override describe(): string {
    return `HttpClient(${this.base})`;
  }
}
