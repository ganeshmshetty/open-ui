# Complete exemplar programs

These 24 programs follow DESIGN §8.1 and are checked by `genui-cli spec-check`.
They use default std spacing, headline Stat grids, Facts detail panels, modest
chart/table layouts, status Badges, scoped forms and verb-labeled actions.

Literal inline records are sample data. A demo host supplies these named refs:

| Ref | Shape |
| --- | --- |
| t4.people | list of {id: string, name: string, location: string} |
| t4.places | list of {id: string, name: string, address: string, photo: host image URL} |
| t4.orders | list of {id: string, customer: string, total: number, status: string} |

The explicit documentation context is `conformance/libs/host.json`; runtime
hosts register their own catalog. Examples with actions produce inert plans in
M2. Rendering, resolving refs, field validation and sending turns require M4 and
the later renderer milestones.
