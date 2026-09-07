import './styles/style.css';
import { EditorController } from './editor/editor-controller';

const root = document.getElementById('editor');

if (!root) {
  throw new Error('The editor document is missing its #editor root.');
}

const controller = new EditorController(
  root,
  globalThis.webkit?.messageHandlers?.carver,
);
controller.initialize();
globalThis.carverEditor = controller;
