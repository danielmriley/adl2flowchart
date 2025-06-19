# ADL Flowchart Generation

## Dependencies

`flex`, `bison`, `graphviz`, and `make` are required.

For linux systems run:

```bash
apt install flex bison graphviz make
```

## To compile

Run `make` and the executeable `smash` will be generated.

To run:

```bash
./smash <FILE>
```

Two files will be made.
`ast.dot` and `fc.dot`

Run:

```bash
dot -Tpdf ast.dot -o ast.pdf
dot -Tpdf fc.dot -o fc.pdf
```

to create the PDFs.
