HTTP Pull Convergence Layer
===========================

**Version: 0.1**

While the most common Bundle Protocol / DTN convergence layers and routing protocols work on a push basis, this CL is based on a pull strategy similar to protocols like [Forban](https://github.com/adulau/Forban).

The CLA does not accept bundles for transmission but periodically checks all peers for the bundles they have in their store. 

For each peer with the default webservice and/or *httppull* CLA the following steps are performed:

1. get hash digest of remote bundle store (`/status/bundles/digest`)
2. compare to own store, if they differ, continue
3. get a list of bundles at the remote node (`/status/bundles`)
4. download the bundles missing from the local store (`/download`)