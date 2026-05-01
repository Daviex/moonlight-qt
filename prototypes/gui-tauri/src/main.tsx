import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import { StreamWindowApp } from './components/StreamWindowApp';
import './styles.css';

const Root = new URLSearchParams(window.location.search).has('streamWindow') ? StreamWindowApp : App;

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
