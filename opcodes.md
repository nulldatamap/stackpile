OPCODE SET (non-strict VM):
NOP       - Do nothing
PUSH V    - Push a 32bit value V onto the stack
POP       - Remove a value from the top of stack
DUP       - Duplicate the top value of the stack
SWAP      - Swap the two top values of the stack
PEEK N    - Copy the Nth value of the stack to the top
REM N     - Remove the NTh value of the stack
YIELD     - Yield the task to another
CALL F    - Call a function F
CALLF F   - Call a foreign function F
IF T E    - If the top of the stack is non-zero execute T, else execute E
SPAWN F   - Spawn a new task with the code F
MAPTOP F  - Map the top value with foreign F
BINOP F   - Pop B, pop A, call foreign F with A and B and push the result

---------------------------------------------------------------------------
Possible addinational instruction:
REP V     - Replaces top with V: POP, PUSH V
PMAPTOP F - Preserves the top values and pushes the mapped one: DUP, MAPTOP F
MAPN N F  - 
RMAPN N F - 
SWAPN N   - 
CALLV     - 

----------------------------------------------------------------------------

STAGE BASED OPS:

Group - Group-name op-code range:
  OPCODE-NAME PARAMS
  - (Byte width) Byte layout
  -  Description

Group - Flow control 0x00-0x1F:
  NOP
  - ( 1)  00
  - Does nothing.

  YIELD      
  - ( 1)  01
  - Yields the current task

  SPAWN F
  - ( 5)  02 FF FF FF FF     
  - Spawn a new task running F

  IF X T E
  - (10)  03 XX TT TT TT TT EE EE EE EE
  - If the value at slot X is non-zero call T, else call E

  CALL X F
  - ( 6)  04 XX FF FF FF FF  
  - Calls the function F with an origin of X

  CALLF X F
  - ( 6)  05 XX FF FF FF FF 
  - Calls a foreign function F with an origin of X

  MAP X F
  - ( 6)  06 XX FF FF FF FF     
  - Applies a foreign map function F to X

  MAPTO X Y F
  - ( 7)  07 XX YY FF FF FF FF 
  - Applies a foreign map fucntion F to X and writes it to Y
 
  BINOP X F
  - ( 6)  08 XX FF FF FF FF
  - Calls a foreign binary function F originating at X
  
  CALLV X V
  - ( 3)  09 XX VV
  - Calls the function located at V with an origin of X


Group - Stage manipulation 0x20-0x4F:
  SWAP X Y
  - ( 3)  20 XX YY 
  - Swaps slot X and Y
  MOVE X Y
  - ( 3)  21 XX YY 
  - Moves the value of slot X to slot Y
  SET  X V
  - ( 3)  22 XX YY
  - Sets the value of slot X to V
  CURSOR O
  - ( 3)  23 OO OO
  - Offsets the current origin ( cursor ) by O

main ->
  print (fib 3)

fib x ->
  if x < 2
  then 1
  else (fib x - 1) + (fib x - 2)

<main>
    SET 0 3
    CALL 0 <fib>
    CALLF 0 <print>

<fib>
    MOVE 0 1
    SET 2 2
    BINOP 1 <u32_lt>
    IF 1 <fib::then::0> <fib::else::0>
<fib::then::0>
    SET 0 1
<fib::else::0>
    MOVE 0 1
    SET 2 1
    BINOP 1 <u32_sub>
    CALL 1 <fib>
    MOVE 0 2
    SET 3 2
    BINOP 2 <u32_sub>
    CALL 2 <fib>
    BINOP 1 <u32_add>
    MOVE 1 0

