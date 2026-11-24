// The HTTP app: an Express app that mounts the books router.

const express = require("express");
const { Loan } = require("../domain/loan");

const app = express();

/**
 * Shows one loan.
 *
 * @param {{ params: { id: string } }} req
 * @param {{ json: (body: unknown) => void }} res
 */
function showLoan(req, res) {
  res.json(new Loan({ id: req.params.id, title: "" }, "nobody"));
}

app.get("/health", (req, res) => {
  res.json({ ok: true });
});
app.get("/loans/:id", showLoan);
app.use("/books", require("./books"));

module.exports = app;
