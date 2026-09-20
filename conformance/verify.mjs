#!/usr/bin/env node
// Compatibility entrypoint for the existing JavaScript conformance implementation.
// The canonical Zed lifecycle checker is conformance/check.sh; keep behavioral
// verification behind that single boundary so contract parity cannot be skipped.
import './check.mjs';
