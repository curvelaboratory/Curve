.. _curve _overview_listeners:

Listener
========
Curve leverages Envoy’s Listener subsystem to streamline connection management for developers. 
By building on Envoy’s robust curve itecture, Curve simplifies the configuration required to bind incoming 
connections from downstream clients and efficiently manages internal listeners for outgoing connections 
to LLM hosts and APIs.

**Listener Subsystem Overview**

- **Downstream Connections**: Curve uses Envoy's Listener subsystem to accept connections from downstream clients. 
  A listener acts as the primary entry point for incoming traffic, handling initial connection setup, including network 
  filtering and security checks, such as SNI and TLS termination. For more details on the listener subsystem, refer to the 
  `Envoy Listener Configuration <https://www.envoyproxy.io/docs/envoy/latest/configuration/listeners/listeners>`_.

- **Internal Listeners for Outgoing Connections**: Curve automatically configures internal listeners to route requests 
  from prompts origination from your application services to appropriate upstream targets, including LLM hosts and backend APIs. 
  This configuration abstracts away complex networking setups, allowing developers to focus on business logic rather than the 
  intricacies of connection management and multiple SDKs to work with different LLM providers.

- **Simplified Configuration**: Curve minimizes the complexity of traditional Envoy setups by pre-defining essential 
  listener settings, making it easier for developers to bind connections without deep knowledge of Envoy’s configuration model. 
  This simplification ensures that connections are secure, reliable, and optimized for performance.

Curve’s dependency on Envoy’s Listener subsystem provides a powerful, developer-friendly interface for managing connections, 
enhancing the overall efficiency of handling prompts and routing them to the correct endpoints within a generative AI application.