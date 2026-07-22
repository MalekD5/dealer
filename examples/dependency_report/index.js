'use strict';

const report = require('./lib/report');

const projectDirectory = __dirname;
const rows = report.collect(projectDirectory);

report.print(rows, projectDirectory);

// Anything declared but missing, or installed outside its range, is a failure
// so the script's exit code is worth checking.
const unsatisfied = rows.filter(function (row) {
  return !row.satisfied;
});

process.exit(unsatisfied.length === 0 ? 0 : 1);
