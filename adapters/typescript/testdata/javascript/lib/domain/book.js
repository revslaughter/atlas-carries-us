// Books and the shelves that hold them. A shelf is a constructor function in
// the older style: its methods live on its prototype.

"use strict";

const path = require("node:path");

/**
 * One book in the catalogue.
 *
 * @typedef {Object} Book
 * @property {BookId} id - The catalogue number.
 * @property {string} title - The title, as printed.
 * @property {number} [copies] - How many copies the library owns.
 */

/** @typedef {string} BookId */

/**
 * @callback BookTest
 * @param {Book} book - The book to test.
 * @returns {boolean}
 */

/** Counting helpers that every shelf gains through a mixin. */
const counting = {
  /** Counts the books. */
  count() {
    return this.books.length;
  },
};

/**
 * A shelf of books, kept in the order they arrive.
 *
 * @constructor
 * @param {string} name - The shelf's label.
 */
function Shelf(name) {
  /** The shelf's label. */
  this.name = name;
  /** @type {Book[]} */
  this.books = [];
  this._sorted = false;
}

/**
 * Adds a book to the end of the shelf.
 *
 * @param {Book} book
 * @returns {Shelf} The shelf, for chaining.
 */
Shelf.prototype.add = function (book) {
  this.books.push(book);
  return this;
};

/**
 * Returns the first book, or undefined on an empty shelf.
 *
 * @returns {Book | undefined}
 */
Shelf.prototype.first = function first() {
  return this.books[0];
};

/**
 * Finds the books that pass a test.
 *
 * @param {BookTest} test
 * @returns {Book[]}
 */
Shelf.prototype.where = function (test) {
  return this.books.filter(test);
};

/**
 * Makes a shelf from a list of books.
 *
 * @param {string} name
 * @param {Book[]} books
 * @returns {Shelf}
 */
Shelf.from = function (name, books) {
  const s = new Shelf(name);
  for (const b of books) s.add(b);
  return s;
};

/** The folder where shelves keep their files. */
Shelf.home = path.join("var", "shelves");

Object.assign(Shelf.prototype, counting);

exports.Shelf = Shelf;
exports.counting = counting;
