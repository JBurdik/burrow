import { createApp } from 'vue';
import { createPinia } from 'pinia';
import { createRouter, createWebHashHistory } from 'vue-router';
import App from './App.vue';
import PairView   from './views/PairView.vue';
import HomeView   from './views/HomeView.vue';
import OutputView from './views/OutputView.vue';

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: '/',               component: PairView   },
    { path: '/home',           component: HomeView   },
    { path: '/output/:ptyId',  component: OutputView },
  ],
});

const app = createApp(App);
app.use(createPinia());
app.use(router);
app.mount('#mobile-app');
