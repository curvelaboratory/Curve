.. _llm_provider:

LLM Provider
============

**LLM provider** is a top-level primitive in Curve, helping developers centrally define, secure, observe,
and manage the usage of of their LLMs. Curve builds on Envoy's reliable `cluster subsystem <https://www.envoyproxy.io/docs/envoy/v1.31.2/intro/curve _overview/upstream/cluster_manager>`_
to manage egress traffic to LLMs, which includes intelligent routing, retry and fail-over mechanisms,
ensuring high availability and fault tolerance. This abstraction also enables developers to seamlessly
switching between LLM providers or upgrade LLM versions, simplifying the integration and scaling of LLMs
across applications.


Below is an example of how you can configure ``llm_providers`` with an instance of an Curve gateway.

.. literalinclude:: includes/curve_config.yaml
    :language: yaml
    :linenos:
    :lines: 1-20
    :emphasize-lines: 10-16
    :caption: Example Configuration

.. Note::
    When you start Curve, it creates a listener port for egress traffic based on the presence of ``llm_providers``
    configuration section in the ``prompt_config.yml`` file. Curve binds itself to a local address such as
    ``127.0.0.1:51001/v1``.

Curve also offers vendor-agnostic SDKs and libraries to make LLM calls to API-based LLM providers (like OpenAI,
Anthropic, Mistral, Cohere, etc.) and supports calls to OSS LLMs that are hosted on your infrastructure. Curve
abstracts the complexities of integrating with different LLM providers, providing a unified interface for making
calls, handling retries, managing rate limits, and ensuring seamless integration with cloud-based and on-premise
LLMs. Simply configure the details of the LLMs your application will use, and Curve offers a unified interface to
make outbound LLM calls.

Example: Using the OpenAI Python SDK
------------------------------------

.. code-block:: python

   from openai import OpenAI

   # Initialize the Curve client
   client = OpenAI(base_url="http://127.0.0.1:51001/v1")

   # Define your LLM provider and prompt
   llm_provider = "openai"
   prompt = "What is the capital of France?"

   # Send the prompt to the LLM through Curve
   response = client.completions.create(llm_provider=llm_provider, prompt=prompt)

   # Print the response
   print("LLM Response:", response)
