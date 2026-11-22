// Package cobra stands in for github.com/spf13/cobra in the adapter's test
// fixture.
package cobra

// Command is one command line command.
type Command struct {
	Use   string
	Short string
	Run   func(cmd *Command, args []string)
	RunE  func(cmd *Command, args []string) error
	subs  []*Command
}

// AddCommand adds sub-commands.
func (c *Command) AddCommand(cmds ...*Command) { c.subs = append(c.subs, cmds...) }

// Execute runs the command.
func (c *Command) Execute() error { return nil }
