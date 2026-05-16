import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import './index.css'
import './renderer/styles/panels.css'

import { applyTheme, defaultTheme } from './themes/themes'

applyTheme(defaultTheme)

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
