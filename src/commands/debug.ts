import { Command } from 'commander';
import { RPCConfigParser } from '../config/rpc-config';
import { FallbackRPCClient } from '../rpc/fallback-client';

export function registerDebugCommand(program: Command): void {
    program
        .command('debug <transaction>')
        .description('Debug a Stellar transaction with RPC fallback support')
        .option(
            '--rpc <urls>',
            'Comma-separated list of RPC URLs (e.g., https://rpc1.com,https://rpc2.com)',
        )
        .option('--timeout <ms>', 'Request timeout in milliseconds', '30000')
        .option('--retries <n>', 'Number of retries per endpoint', '3')
        .action(async (transaction: string, options) => {
            try {
                // Load RPC configuration
                const config = RPCConfigParser.loadConfig({
                    rpc: options.rpc,
                    timeout: parseInt(options.timeout),
                    retries: parseInt(options.retries),
                });

                // Initialize RPC client with fallback
                const rpcClient = new FallbackRPCClient(config);

                // Make RPC request
                console.log(`\n🔍 Debugging transaction: ${transaction}\n`);

                // Note: In a real app, this would be a real API path
                const txData = await rpcClient.request('/transactions/' + transaction);

                console.log('Transaction data:', JSON.stringify(txData, null, 2));

            } catch (error) {
                if (error instanceof Error) {
                    console.error('❌ Debug failed:', error.message);
                } else {
                    console.error('❌ Debug failed: An unknown error occurred');
                }
                process.exit(1);
            }
        });

    // Add health check command
    program
        .command('rpc:health')
        .description('Check health of all configured RPC endpoints')
        .option('--rpc <urls>', 'Comma-separated list of RPC URLs')
        .action(async (options) => {
            try {
                const config = RPCConfigParser.loadConfig({ rpc: options.rpc });
                const rpcClient = new FallbackRPCClient(config);

                await rpcClient.performHealthChecks();

                const status = rpcClient.getHealthStatus();

                console.log('\n📊 RPC Endpoint Status:\n');
                status.forEach((ep, idx) => {
                    const statusIcon = ep.healthy ? '✅' : '❌';
                    const circuit = ep.circuitOpen ? ' [CIRCUIT OPEN]' : '';
                    console.log(`  [${idx + 1}] ${statusIcon} ${ep.url}${circuit}`);
                    console.log(`      Failures: ${ep.failureCount}`);
                });

            } catch (error) {
                if (error instanceof Error) {
                    console.error('❌ Health check failed:', error.message);
                } else {
                    console.error('❌ Health check failed: An unknown error occurred');
                }
                process.exit(1);
            }
        });
}
