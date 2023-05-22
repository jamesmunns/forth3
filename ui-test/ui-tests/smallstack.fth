( NOTE: This test shows that we properly handle stack overflow cases )
( We use the frontmatter to set the max stack size to four elements: )

( data_stack_elems 4 )

> 1 2 3 4
> .s
< <4> 1 2 3 4
< ok.
x 5
> .s
< <0>
< ok.
