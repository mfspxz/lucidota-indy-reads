// Receipt stream for the Indy_Reads demo backend. Hash-chained, per instance.
module.exports = async (req, res) => {
  // The chain lives in turn.js's module scope; this endpoint reports honestly
  // that per-instance receipts reset on cold start.
  res.json({ receipts: [], note: 'per-instance chain; see turn responses for receipt ids' });
};
