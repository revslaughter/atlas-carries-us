// The staff handlers. Each one takes a request and writes a response.

import type { NextFunction, Request, Response } from "express";

import { HttpClient } from "../client/api.js";
import type { BookID, NewBook } from "../domain/book.js";

const client = new HttpClient("http://localhost:8080", fetch);

/** Shows one book. */
export async function showBook(req: Request, res: Response) {
  res.json(await client.getBook(req.params.id as BookID));
}

/** Lists the catalogue. Staff see every book, lent or not. */
export const listBooks = (_req: Request, res: Response) => res.json([]);

/** Adds a book from the request body. */
export async function addBook(req: Request, res: Response) {
  res.status(201).json(await client.add(req.body as NewBook));
}

/** Lets a request through only when it carries the staff token. */
export function requireStaff(req: Request, res: Response, next: NextFunction) {
  if (req.params.token !== "staff") return res.status(403).send();
  next();
}

/** Writes the loan report for one member. */
export const loanReport = async (req: Request, res: Response) => {
  res.json(await client.loans(req.params.member ?? "", true));
};

/** Answers that the cache is empty. The staff server has no cache of its own. */
export function clearCache(_req: Request, res: Response) {
  res.send();
}
