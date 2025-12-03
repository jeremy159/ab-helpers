import { createRequire } from "module";
const require = createRequire(import.meta.url);

// Import the CommonJS package once
const api = require("@actual-app/api");

// Re-export for ES6 modules
export default api;
