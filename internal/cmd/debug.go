package cmd

import (
	"fmt"

	"github.com/dotandev/hintents/internal/localization"
	"github.com/dotandev/hintents/internal/rpc"
	"github.com/spf13/cobra"
)

var (
	networkFlag string
	rpcURLFlag  string
)

var debugCmd = &cobra.Command{
	Use:   "debug <transaction-hash>",
	Short: localization.Get("cli.debug.short"),
	Long:  localization.Get("cli.debug.long"),
	Args:  cobra.ExactArgs(1),
	PreRunE: func(cmd *cobra.Command, args []string) error {
		switch rpc.Network(networkFlag) {
		case rpc.Testnet, rpc.Mainnet, rpc.Futurenet:
			return nil
		default:
			return fmt.Errorf(localization.Get("error.invalid_network"), networkFlag)
		}
	},
	RunE: func(cmd *cobra.Command, args []string) error {
		txHash := args[0]

		client := rpc.NewClient(rpc.Network(networkFlag))
		if rpcURLFlag != "" {
			client = rpc.NewClientWithURL(rpcURLFlag, rpc.Network(networkFlag))
		}

		resp, err := client.GetTransaction(cmd.Context(), txHash)
		if err != nil {
			return fmt.Errorf(localization.Get("error.fetch_transaction"), err)
		}

		fmt.Printf(localization.Get("output.transaction_envelope")+"\n", len(resp.EnvelopeXdr))
		return nil
	},
}

func init() {
	debugCmd.Flags().StringVarP(&networkFlag, "network", "n", string(rpc.Mainnet), localization.Get("cli.debug.flag.network"))
	debugCmd.Flags().StringVar(&rpcURLFlag, "rpc-url", "", localization.Get("cli.debug.flag.rpc_url"))

	rootCmd.AddCommand(debugCmd)
}
