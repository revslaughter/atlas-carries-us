// An invented stand-in for the types of the express package, so that the
// fixture type-checks without node_modules. It declares only what the
// fixture uses.

declare function express(): express.Express;

declare namespace express {
  interface Request {
    params: Record<string, string | undefined>;
    body: unknown;
  }
  interface Response {
    json(body: unknown): Response;
    status(code: number): Response;
    send(body?: unknown): Response;
  }
  type NextFunction = (err?: unknown) => void;
  type RequestHandler = (req: Request, res: Response, next: NextFunction) => unknown;
  interface IRoute {
    get(...handlers: RequestHandler[]): IRoute;
    post(...handlers: RequestHandler[]): IRoute;
  }
  interface IRouter {
    get(path: string, ...handlers: RequestHandler[]): this;
    post(path: string, ...handlers: RequestHandler[]): this;
    delete(path: string, ...handlers: RequestHandler[]): this;
    use(...args: unknown[]): this;
    route(path: string): IRoute;
  }
  interface Router extends IRouter {}
  interface Express extends IRouter {
    listen(port: number): void;
  }
  function Router(): Router;
  function json(): RequestHandler;
}

export = express;
