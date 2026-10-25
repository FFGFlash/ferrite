import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './app/App'
import { installGlobalErrorLogging } from './ipc'
import { followSystemTheme } from './themes/theme'
import './styles/tokens.css'
import './styles/base.css'
import './styles/accessibility.css'

followSystemTheme()
installGlobalErrorLogging()

const root = document.getElementById('root')
if (!root) {
  throw new Error('Missing #root element')
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
