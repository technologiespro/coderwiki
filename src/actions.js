import { store } from './store';
import { fetchDocs, fetchRepos, deleteRepoApi } from './api';

export async function loadDocs(repoKey) {
  const record = await fetchDocs(repoKey);
  store.docs = record;
  store.selectedFile = null;
  store.selectedSection = null;
  localStorage.setItem('lastRepoKey', repoKey);
}

export async function deleteRepo(repoKey) {
  await deleteRepoApi(repoKey);
  if (store.docs && store.docs.repoKey === repoKey) {
    store.docs = null;
    store.selectedFile = null;
    store.selectedSection = null;
    localStorage.removeItem('lastRepoKey');
  }
  await loadRepos();
}

export async function loadRepos() {
  try {
    const data = await fetchRepos();
    store.repos = data.repos || [];
  } catch {
    store.repos = [];
  }
}
