# Core Features

## Display crypto prices

### Download prices from the Internet
- setup STM32 ethernet driver
- setup TCP/IP stack
- http(s) client

### Display the prices

### Device uses a crypto market API to download current prices

## Update the prices
- Update the prices every 30s

## Picking the displayed cryptos

### Expose API that allows configuration of selected cryptocurrency variants
- setup tcp socket
- http server
- save configuration to flash memory

## Display driver
- setup embedded graphics drawable driver
- invent an abstract layout
- The screen doesn't flicker
- In case of error display a message

## Definition of done
- Device's resolution is 192x96
- Device displays colors (4 bits per color depth)

## Works out of the box
- Has default configuration
- Reads MAC address from memory
- Uses DHCP
- Uses NTP

### Bonus
-Display current crypto wallet contents
