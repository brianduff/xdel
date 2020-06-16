Finds android resource references, and manipulates them.

Example usage:
```
# Build indices (speed up subsequent commands, required).
xdel -j java -r res index
 
# Counts defined, used, and unused string resources:
xdel -j java -r res counts
 
# Lists unused string resources
xdel -j java -r res ls-unused
 
# Lists unused string resources with definition locations
xdel -j java -r res ls-unused -s
 
# Deletes all references to unused string resources with the prefix foo_
xdel -j java -r res rm-unused -p foo_
```