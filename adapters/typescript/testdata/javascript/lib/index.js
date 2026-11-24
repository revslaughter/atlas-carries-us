/**
 * @file The lending library's front door. It gathers the shelves, the loans
 * and the rules, and exports them as one object.
 */

const { Shelf } = require("./domain/book");
const { Loan } = require("./domain/loan");
const rules = require("./domain/rules");

/**
 * Opens a shelf with the default rules.
 *
 * @param {string} name - The shelf's name.
 * @returns {Shelf}
 */
function openShelf(name) {
  return new Shelf(name);
}

module.exports = {
  Shelf,
  Loan,
  rules,
  openShelf,
  /** Lends the first book of a shelf to a member. */
  lendFirst(shelf, member) {
    return new Loan(shelf.first(), member);
  },
  version: "1.0.0",
};
