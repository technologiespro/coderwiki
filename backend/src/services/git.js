import { spawn } from 'child_process';
import fs from 'fs/promises';
import path from 'path';

export function parseRepoUrl(repoUrl) {
  const m = String(repoUrl).trim().match(/github\.com[/:]([\w.-]+)\/([\w.-]+?)(?:\.git)?\/?$/i);
  if (!m) {
    const err = new Error('Некорректный GitHub URL. Ожидается https://github.com/owner/repo');
    err.status = 400;
    throw err;
  }
  return { owner: m[1], name: m[2], key: `${m[1]}/${m[2]}` };
}

export async function cloneRepo(repoUrl, targetDir, githubToken) {
  const { owner, name } = parseRepoUrl(repoUrl);
  const cloneUrl = `https://github.com/${owner}/${name}.git`;
  await fs.rm(targetDir, { recursive: true, force: true });
  await fs.mkdir(path.dirname(targetDir), { recursive: true });
  const env = { ...process.env, GIT_TERMINAL_PROMPT: '0' };
  if (githubToken) {
    // PAT через одноразовый http.extraHeader: токен не попадает в URL, логи и .git/config
    const auth = Buffer.from(`x-access-token:${githubToken}`).toString('base64');
    env.GIT_CONFIG_COUNT = '1';
    env.GIT_CONFIG_KEY_0 = 'http.extraHeader';
    env.GIT_CONFIG_VALUE_0 = `Authorization: Basic ${auth}`;
  }
  await new Promise((resolve, reject) => {
    const proc = spawn('git', ['clone', '--depth', '1', cloneUrl, targetDir], { env });
    let stderr = '';
    proc.stderr.on('data', (d) => (stderr += d));
    proc.on('error', reject);
    proc.on('close', (code) => {
      if (code === 0) resolve();
      else reject(new Error(`git clone failed: ${stderr.slice(-400)}`));
    });
  });
  return targetDir;
}
