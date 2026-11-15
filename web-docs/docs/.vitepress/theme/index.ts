import DefaultTheme from 'vitepress/theme';
import Layout from './Layout.vue';
import RepoShowcase from './components/RepoShowcase.vue';
import './style.css';

export default {
  extends: DefaultTheme,
  Layout,
  enhanceApp({ app }) {
    app.component('RepoShowcase', RepoShowcase);
  },
};
