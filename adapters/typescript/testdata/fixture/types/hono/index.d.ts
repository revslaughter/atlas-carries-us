// An invented stand-in for the types of the hono package, so that the
// fixture type-checks without node_modules. It declares only what the
// fixture uses.

export interface Context {
  req: { param(name: string): string; json<T>(): Promise<T> };
  json(body: unknown, status?: number): Response;
  text(body: string): Response;
}

export type Handler = (c: Context, next: () => Promise<void>) => Response | Promise<Response>;

export declare class Hono {
  get(path: string, ...handlers: Handler[]): Hono;
  post(path: string, ...handlers: Handler[]): Hono;
  on(method: string | string[], path: string | string[], ...handlers: Handler[]): Hono;
  route(path: string, app: Hono): Hono;
  basePath(path: string): Hono;
}

export declare function serveStatic(options: { root: string }): Handler;
