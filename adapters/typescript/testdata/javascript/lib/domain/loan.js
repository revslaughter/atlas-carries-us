// Loans: an ES class with its fields set in the constructor, and a subclass.

const rules = require("./rules");

/**
 * A loan of one book to one member.
 */
class Loan {
  /** The number of loans made so far. */
  static made = 0;

  #returned = false;

  /**
   * Starts a loan.
   *
   * @param {import('./book').Book} book - The book on loan.
   * @param {string} member - Who borrows it.
   */
  constructor(book, member) {
    /** The book on loan. */
    this.book = book;
    /** @type {string} */
    this.member = member;
    /** The day the loan falls due, counted from day 0. */
    this.due = rules.loanDays;
    Loan.made++;
  }

  /** Whether the member has brought the book back. */
  get returned() {
    return this.#returned;
  }

  /**
   * Ends the loan.
   *
   * @returns {void}
   */
  close() {
    this.#returned = true;
    this.#log("closed");
  }

  /**
   * Reports whether the loan runs past its due day.
   *
   * @param {number} today
   * @returns {boolean}
   */
  isLate(today) {
    return rules.isLate(this, today);
  }

  /**
   * Writes one line about the loan.
   *
   * @private
   * @param {string} what
   */
  #log(what) {
    this.lastEvent = what;
  }
}

/**
 * A loan that already runs late. {@link Loan.isLate} always holds for it.
 */
class OverdueLoan extends Loan {
  /** @param {number} today */
  isLate(today) {
    return today >= 0;
  }
}

/**
 * Picks the first item of a list that passes a test.
 *
 * @template T
 * @param {T[]} items
 * @param {(item: T) => boolean} test
 * @returns {T | undefined}
 */
function pick(items, test) {
  return items.find(test);
}

module.exports = { Loan, OverdueLoan, pick };
