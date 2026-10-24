    # SigmaOS-dev-tools

    Developer toolchain helpers

    Part of the **SigmaOS** sovereign operating-system ecosystem — a multi-repository
    distribution modelled on Linux Mint and Omarchy Linux. See the parent
    [`SigmaOS`](https://github.com/AaryanSinghChauhan09/SigmaOS) repository for the
    full component index.

    ## Upstream inspirations
    - https://github.com/linuxmint/mint-dev-tools

    ## Design principles applied
    - **Plan Before Apply**
- **Pure Functions**
- **Fail-Fast**
- **KISS**
- **Deterministic Testing**

    ## Usage
    Verbs: `doctor, fmt-plan, lint-plan, bootstrap`

    ```
    $ sigma-devtools doctor --tools tools.json
$ sigma-devtools fmt-plan --languages python,shell,markdown
$ sigma-devtools lint-plan --languages python --strict
$ sigma-devtools bootstrap --stack python-web --package-manager apt

    ```

    ## Build & test
    ```
    make build
    make test
    ```

    ## Licence
    GNU General Public License v3.0 (FOSS).
