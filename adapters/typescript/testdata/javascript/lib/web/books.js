// The books router, which the app mounts under /books.

const { Router } = require("express");
const { Shelf } = require("../domain/book");
const rules = require("../domain/rules");

const router = Router();
const shelf = new Shelf("front desk");

/**
 * Lists the books on the front desk.
 *
 * @param {unknown} req
 * @param {{ json: (body: unknown) => void }} res
 */
function listBooks(req, res) {
  res.json(shelf.where(b => !!b.title));
}

router.get("/", listBooks);
router.post("/:id/loans", function borrow(req, res) {
  res.json({ ok: rules.mayBorrow(shelf.count()) });
});

module.exports = router;
