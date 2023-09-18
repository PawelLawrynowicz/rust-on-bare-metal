# Problems with the DICE project

## RTIC
1.  RTIC reserves systick for scheduling.
	Because of that it's impossible to use systick for anything else, as RTIC blocks
	access to the SYST peripheral.

2.	Generic types are not allowed in RTIC resources struct. 
	This feature may be added in v0.6.0

3.  RTIC 0.6.0alpha changes a lot of things, scheduler calls included. I couldn't compile it because for some reason new scheduling method didn't work. I suggest we stick to 0.5 for now and upgrade to 0.6.0 when a stable version is released.

4.  RTIC allows conditional fields in resource struct, but still requires types, dependencies, etc. of those fields even if they should not be compiled.

	Example:

	```rust

	    [cfg(feature="target1")] 
	    use target1::foo;

	    struct Resources{
			var1: u8,
			[cfg(feature="target1")] 
			var2: foo,
		}  

	```

	cargo build without features=["target1"] results in foo not in scope error.


## SmolTcp
1.  The current smoltcp implementation of Tcp sockets cannot be used to properly implement embedded_nal 0.4.0 TcpFullStack interface.

## Other
1. Forcing write to stm32 flash doesn't work. The buffer is cleared, but the value is not written to memory. After calling force write function the FW1 bit is not set (documentation page 201). Effectively we are not able to write less than 256 bit words to the memory. May be a bug in HAL or we're doing something wrong.

2. probe-run reports problem with communication to STLINK, when the MCU is running program that modifies flash (including erasing). The authors are aware of the issue but there's no solution for STLINK at the moment.
https://github.com/knurling-rs/defmt/issues/140

3. Cannot perform any math operations on const generics. This may cause problems with implementing hub75 driver for displays with multiplexing.

	Example:

	```rust

		const NUM_ROWS = 16;

	    pub struct Hub75<const ROW_LENGTH: usize> {
    		#[cfg(not(feature = "stripe-multiplexing"))]
    		data: [[(u8, u8, u8, u8, u8, u8); ROW_LENGTH]; NUM_ROWS],
    		#[cfg(feature = "stripe-multiplexing")]
    		data: [[(u8, u8, u8, u8, u8, u8); ROW_LENGTH * 2]; NUM_ROWS / 2],
			//this will cause compile error ->^^^^^^^^^^^^^^   ^^^^^^^^^^^^^ <- but this is fine
		}

	```



