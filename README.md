##ADL Flowchart Generation

Run `make` and the executeable `smash` will be generated.

To run:

```
./smash <FILE>
```

Two files will be made.
`ast.dot` and `fc.dot`

Run:

```
dot -Tpdf ast.dot -o ast.pdf
dot -Tpdf fc.dot -o fc.pdf
```

to create the appropriate PDF.
