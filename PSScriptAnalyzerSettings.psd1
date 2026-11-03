# PSScriptAnalyzer rules for the repo's PowerShell scripts (`npm run lint:ps`, CI).
@{
    Severity     = @('Error', 'Warning')
    ExcludeRules = @(
        # Interactive setup/release scripts print to the console on purpose.
        'PSAvoidUsingWriteHost',
        # Best-effort probes (version strings, cache stats) ignore failures.
        'PSAvoidUsingEmptyCatchBlock',
        # Naming style only; renaming helpers buys nothing.
        'PSUseSingularNouns',
        'PSUseApprovedVerbs',
        # Misfires on script parameters read inside functions ($Yes) and on
        # parameters kept for CLI parity with the .sh twin ($OS).
        'PSReviewUnusedParameter'
    )
}
