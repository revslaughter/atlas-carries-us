// The lending rules, as an object literal that acts as a module.

/** The rules that every loan follows. */
const rules = {
  /** How many days a loan lasts. */
  loanDays: 14,
  /**
   * Reports whether a loan runs late.
   *
   * @param {{ due: number }} loan
   * @param {number} today
   * @returns {boolean}
   */
  isLate(loan, today) {
    return today > loan.due;
  },
  /** Counts the days a loan runs late, or 0. */
  daysLate: (loan, today) => Math.max(0, today - loan.due),
};

/**
 * Reports whether a member may borrow one more book.
 *
 * @param {number} onLoan - The member's books on loan now.
 * @returns {boolean}
 */
rules.mayBorrow = function (onLoan) {
  return onLoan < rules.limit;
};

/** The most books a member may hold at once. */
rules.limit = 3;

module.exports = rules;
