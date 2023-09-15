
import { createApp } from 'vue'
import { createPinia } from 'pinia'

// @ts-ignore
import App from './App.vue'
import router from './router'

import PrimeVue from 'primevue/config'
import ToastService from 'primevue/toastservice'
import DialogService from 'primevue/dialogservice'

import '@/assets/styles.scss'
import Ripple from "primevue/ripple";
import StyleClass from "primevue/styleclass";

const app = createApp(App)

app.use(createPinia())
app.use(router)
app.use(PrimeVue, {ripple: true})
app.use(ToastService);
app.use(DialogService);

app.directive('ripple', Ripple);
app.directive('styleclass', StyleClass);

app.mount('#app')
