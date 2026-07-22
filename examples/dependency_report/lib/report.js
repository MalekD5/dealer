'use strict';

const fs = require('fs');
const path = require('path');

const chalk = require('chalk');
const semver = require('semver');

/**
 * A report about missing dependencies is not much use if it cannot run when
 * one is missing, so anything inessential is loaded defensively.
 */
function optionalRequire(name) {
  try {
    return require(name);
  } catch (error) {
    return null;
  }
}

const ms = optionalRequire('ms');

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, 'utf8'));
}

/**
 * Pairs every dependency the manifest declares with the version dealer
 * actually put in node_modules.
 */
function collect(projectDirectory) {
  const manifest = readJson(path.join(projectDirectory, 'package.json'));
  const declared = Object.assign({}, manifest.dependencies, manifest.devDependencies);

  return Object.keys(declared)
    .sort()
    .map(function (name) {
      const installedManifest = path.join(projectDirectory, 'node_modules', name, 'package.json');
      const installed = fs.existsSync(installedManifest)
        ? readJson(installedManifest).version
        : null;

      return {
        name: name,
        range: declared[name],
        installed: installed,
        satisfied: installed !== null && semver.satisfies(installed, declared[name])
      };
    });
}

/** How long ago the install happened, in words. */
function installedAgo(projectDirectory) {
  const nodeModules = path.join(projectDirectory, 'node_modules');
  if (ms === null || !fs.existsSync(nodeModules)) {
    return null;
  }

  const elapsed = Date.now() - fs.statSync(nodeModules).mtimeMs;

  return ms(Math.round(elapsed), { long: true });
}

function print(rows, projectDirectory) {
  const widest = rows.reduce(function (widest, row) {
    return Math.max(widest, row.name.length);
  }, 0);

  console.log(chalk.bold('dependency report'));
  console.log('');

  rows.forEach(function (row) {
    const name = chalk.cyan(row.name.padEnd(widest));
    const range = chalk.dim(row.range.padEnd(10));

    if (row.installed === null) {
      console.log('  ' + chalk.red('x') + ' ' + name + ' ' + range + chalk.red('not installed'));
      return;
    }

    const mark = row.satisfied ? chalk.green('v') : chalk.yellow('!');
    const version = row.satisfied ? chalk.green(row.installed) : chalk.yellow(row.installed);
    const note = row.satisfied ? '' : chalk.yellow(' (outside the declared range)');

    console.log('  ' + mark + ' ' + name + ' ' + range + version + note);
  });

  const age = installedAgo(projectDirectory);
  if (age !== null) {
    console.log('');
    console.log(chalk.dim('node_modules written ' + age + ' ago'));
  }
}

module.exports = { collect: collect, print: print, installedAgo: installedAgo };
