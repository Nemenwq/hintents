#!/usr/bin/env node
import { Command } from 'commander';
import { registerProtocolCommands } from './commands/protocol-handler';
import { registerDebugCommand } from './commands/debug';

const program = new Command();

program
    .name('erst')
    .description('Error Recovery and Simulation Tool (ERST) for Stellar')
    .version('1.0.0');

// Register commands
registerProtocolCommands(program);
registerDebugCommand(program);

program.parse(process.argv);

// If no arguments provided, show help
if (!process.argv.slice(2).length) {
    program.outputHelp();
}
