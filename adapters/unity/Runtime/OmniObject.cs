using System.Collections.Generic;
using UnityEngine;

namespace Omnigenesys
{
    /// <summary>
    /// Put on every prefab instance the builder makes for a map object: its
    /// id, type, box and props, as the map has them.
    /// </summary>
    public sealed class OmniObject : MonoBehaviour
    {
        public OmniObjectData Data { get; internal set; }

        /// <summary>A prop by name, or null.</summary>
        public OmniValue Prop(string name) =>
            Data != null && Data.Props.TryGetValue(name, out var v) ? v : null;
    }

    /// <summary>
    /// A component on a prefab that wants its map object: called once every
    /// object of the map exists, so a prop naming another object (type
    /// <c>object</c>) can be resolved through <paramref name="spawned"/>.
    /// </summary>
    public interface IOmniSpawned
    {
        void OnOmniSpawned(OmniObject self, OmniMap map, IReadOnlyDictionary<long, OmniObject> spawned);
    }
}
