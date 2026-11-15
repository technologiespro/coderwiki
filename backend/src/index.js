import 'dotenv/config';
import express from 'express';
import cors from 'cors';
import apiRouter from './routes/api.js';

const app = express();
app.use(cors({ origin: process.env.CORS_ORIGINS || '*' }));
app.use(express.json({ limit: '50mb' }));
app.use('/api', apiRouter);

app.use((err, req, res, next) => {
  res.status(err.status || 500).json({ error: err.message || 'Internal server error' });
});

const port = process.env.PORT;
app.listen(port, '0.0.0.0', () => {
  console.log(`CoderWiki backend listening on 0.0.0.0:${port}`);
});
