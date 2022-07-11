## Goal

Streamline the process of opening new PRs as follows:

### Setup

1. Specify a template for a PR (template can be hardcoded for a v1)

### Running a binary

1. Takes an argument for the branch to open a PR against (defaults to `master` or `main`)
2. Parses template for a PR / configuration for tool
3. Parses the name of the current git branch
4. Parses commits that current branch has that target branch does not
5. Performs **actions** according to all data available
6. Fills out the template with the data
7. Opens up a text editor for one or more prompts to fill in relevant parts of the PR
8. Opens a PR on GitHub

## Initial Version

1. Parse `dit-xxx` from the name of the current branch and fetch the Linear issue from it. Include the link to the Linear ticket under `Context` along with the ticket description.
2. Determine PR title as `[DIT-XXX] {Project Name}: {Issue Title}`
3. Parse the difference in commits between current branch and target branch and populate `Description` with those as bullet points
4. Put everything into an editor and allow the user to edit and save
5. Allow user to select from a list of reviewers
6. Hit `Confirm` (or CR) and open the PR on the specified repository
